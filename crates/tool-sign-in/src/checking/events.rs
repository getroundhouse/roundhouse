use std::mem;

/// A stream of server-sent events, read for their data as its bytes come.
#[derive(Default)]
pub(super) struct Events {
    line: Vec<u8>,
    /// Whether the line before ended in a CR, so an LF is of its end.
    after_cr: bool,
    /// The data of the event so far, if it has any.
    data: Option<Vec<u8>>,
}

impl Events {
    /// Takes the next byte, and gives the data of the event that ends with it, if one does.
    pub(super) fn take(&mut self, byte: u8) -> Option<Vec<u8>> {
        let after_cr = mem::replace(&mut self.after_cr, byte == b'\r');
        match byte {
            b'\n' if after_cr => None,
            b'\n' | b'\r' => self.end_line(),
            _ => {
                self.line.push(byte);
                None
            }
        }
    }

    /// An empty line ends an event. Of the rest, only its data is read, which each line of it adds a line to.
    fn end_line(&mut self) -> Option<Vec<u8>> {
        let line = mem::take(&mut self.line);
        if line.is_empty() {
            return self.data.take();
        }
        let (field, value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => (&line[..colon], &line[colon + 1..]),
            None => (&line[..], &[][..]),
        };
        if field == b"data" {
            let value = value.strip_prefix(b" ").unwrap_or(value);
            match &mut self.data {
                Some(data) => data.extend(b"\n".iter().chain(value)),
                None => self.data = Some(value.to_vec()),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::Events;

    fn read(stream: &str) -> Vec<String> {
        let mut events = Events::default();
        let read = stream.bytes().filter_map(|byte| events.take(byte));
        read.map(|data| String::from_utf8(data).unwrap()).collect()
    }

    #[test]
    fn each_events_data_is_read_however_its_lines_end_and_nothing_else_of_it() {
        for end in ["\n", "\r\n", "\r"] {
            let stream = ["event: message", "id: 1", "data: one", "", ": a comment", "data:two", "data: lines", "", ""];
            let more = ["retry: 5", "", "data", "", "dataset: not data", "", "data: cut off"];

            let read = read(&stream.iter().chain(&more).copied().collect::<Vec<_>>().join(end));

            assert_eq!(read, ["one", "two\nlines", ""], "{end:?}");
        }
    }
}
