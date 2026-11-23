//! A video decoded on a thread of its own, a few frames ahead of what's shown, each turned into BGRA there, so whatever
//! shows it only draws. Whatever shows it keeps the time, takes each frame when it's due, and says where to go.

use crate::bgra::Rows;
use crate::webm::{OpenError, WebmVideo};
use std::io::{self, Read, Seek};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::time::Duration;

/// What the player's thread has done, in order.
#[derive(Debug)]
pub enum Decoded {
    Frame(Shown),
    /// There are no more frames after the last of `seek`'s.
    End {
        seek: u64,
    },
    /// The file couldn't be opened, after which nothing more comes, whatever's asked.
    Unopened,
    /// The file broke where the next frame of `seek` should be, after which nothing more comes until a seek.
    Failed {
        seek: u64,
    },
}

/// A frame, ready to draw.
#[derive(Debug)]
pub struct Shown {
    /// Which seek it follows, 0 for none, so a frame decoded before a seek is known and passed over.
    pub seek: u64,
    pub time: Duration,
    pub width: u32,
    pub height: u32,
    /// Blue, green, red and alpha, a byte each, row after row.
    pub bgra: Vec<u8>,
}

enum Command {
    Seek { time: Duration, seek: u64 },
}

/// A video being decoded. Dropping it stops its thread, once the thread next looks.
pub struct Player {
    commands: Sender<Command>,
    decoded: Receiver<Decoded>,
    seeks: u64,
}

impl Player {
    /// Opens the video `open` gives, on a thread of its own, which goes on to decode its frames from the start, at most
    /// `ahead` beyond those taken. `woken` is called each time there's something to take, from that thread. Fails only
    /// if the system won't start a thread.
    pub fn start<R: Read + Seek>(
        open: impl FnOnce() -> Result<WebmVideo<R>, OpenError> + Send + 'static,
        ahead: usize,
        woken: impl Fn() + Send + 'static,
    ) -> io::Result<Player> {
        let (commands, asked) = mpsc::channel();
        let (sent, decoded) = mpsc::sync_channel(ahead);
        std::thread::Builder::new().name("video".into()).spawn(move || match open() {
            Ok(video) => decode(video, &asked, &sent, &woken),
            Err(_) => {
                if sent.send(Decoded::Unopened).is_ok() {
                    woken();
                }
            }
        })?;
        Ok(Player { commands, decoded, seeks: 0 })
    }

    /// Goes to `time`, so the frames that come next are from there on, and returns the seek they follow, which any
    /// frame taken before them doesn't.
    pub fn seek(&mut self, time: Duration) -> u64 {
        self.seeks += 1;
        let _ = self.commands.send(Command::Seek { time, seek: self.seeks });
        self.seeks
    }

    /// What the thread has done since this was last asked, if anything.
    pub fn take(&self) -> Option<Decoded> {
        self.decoded.try_recv().ok()
    }
}

/// Decodes frames until the player's dropped, going where it's asked to between them, and resting once there are no
/// more until it's asked to go somewhere else.
fn decode<R: Read + Seek>(
    mut video: WebmVideo<R>,
    asked: &Receiver<Command>,
    sent: &SyncSender<Decoded>,
    woken: &impl Fn(),
) {
    let (mut seek, mut resting, mut rows) = (0, false, Rows::default());
    loop {
        let command = if resting { asked.recv().ok() } else { asked.try_recv().ok() };
        if resting && command.is_none() {
            return;
        }
        let went = command.map(|Command::Seek { time, seek: asked }| {
            seek = asked;
            video.seek(time)
        });
        let next = match went {
            Some(Err(e)) => Err(e),
            Some(Ok(())) | None => video.next_frame(),
        };
        let said = match next {
            Ok(Some(frame)) => {
                let mut bgra = Vec::new();
                frame.picture.to_bgra(&mut rows, &mut bgra);
                let (width, height) = (frame.picture.width, frame.picture.height);
                let (width, height) = (u32::try_from(width).unwrap_or(0), u32::try_from(height).unwrap_or(0));
                Decoded::Frame(Shown { seek, time: frame.time, width, height, bgra })
            }
            Ok(None) => Decoded::End { seek },
            Err(_) => Decoded::Failed { seek },
        };
        resting = !matches!(said, Decoded::Frame(_));
        if sent.send(said).is_err() {
            return;
        }
        woken();
    }
}
