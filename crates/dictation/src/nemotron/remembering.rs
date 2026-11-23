//! An encoder that remembers what it made of the inputs it has been given, so the speech eval can hear one recording
//! with several sets of hint words and encode it once. An encoder never sees the hint words, which only its decoding
//! is biased by, and gives the same outputs for the same inputs, so what's remembered is what it would make again.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use parking_lot::Mutex;

use super::parts::{Input, Part, Tensor};
use crate::EngineError;

/// How many bytes of outputs are remembered, at most: a few recordings' worth, since a live window's encoding is
/// about a third of a megabyte, and the eval hears each recording's conditions one after another.
const REMEMBERED_BYTES: usize = 256 << 20;

/// `part`, with what it made of its latest inputs remembered.
pub(super) struct Remembering {
    part: Box<dyn Part>,
    remembered: Mutex<Remembered>,
}

#[derive(Default)]
struct Remembered {
    outputs: HashMap<Key, Vec<Tensor>>,
    /// The keys, oldest first, so the oldest go once there are too many bytes.
    order: VecDeque<Key>,
    bytes: usize,
}

/// A call's inputs and the outputs it asks for, as two hashes of them, each 64 bits and seeded apart, so two calls
/// are taken as the same only if both hashes are.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key(u64, u64);

impl Remembering {
    pub(super) fn new(part: Box<dyn Part>) -> Self {
        Self { part, remembered: Mutex::new(Remembered::default()) }
    }
}

impl Part for Remembering {
    fn run(&self, inputs: &[Input<'_>], outputs: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        let key = Key(hash(0, inputs, outputs), hash(1, inputs, outputs));
        if let Some(made) = self.remembered.lock().outputs.get(&key) {
            return Ok(made.clone());
        }
        let made = self.part.run(inputs, outputs)?;
        self.remembered.lock().keep(key, made.clone());
        Ok(made)
    }
}

impl Remembered {
    fn keep(&mut self, key: Key, made: Vec<Tensor>) {
        self.bytes += bytes(&made);
        self.outputs.insert(key, made);
        self.order.push_back(key);
        while self.bytes > REMEMBERED_BYTES {
            let Some(oldest) = self.order.pop_front() else { break };
            if let Some(gone) = self.outputs.remove(&oldest) {
                self.bytes -= bytes(&gone);
            }
        }
    }
}

fn bytes(made: &[Tensor]) -> usize {
    made.iter().map(|tensor| tensor.values.len() * size_of::<f32>()).sum()
}

fn hash(seed: u8, inputs: &[Input<'_>], outputs: &[&str]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    seed.hash(&mut hasher);
    for input in inputs {
        match input {
            Input::Floats { name, values, shape } => {
                (0u8, name, shape).hash(&mut hasher);
                values.iter().for_each(|value| value.to_bits().hash(&mut hasher));
            }
            Input::Ints { name, values, shape } => (1u8, name, shape, values).hash(&mut hasher),
        }
    }
    outputs.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::Remembering;
    use crate::EngineError;
    use crate::nemotron::parts::{Input, Part, Tensor};

    /// An encoder whose output is the sum of its features and its prompt, so each input has an answer of its own,
    /// and that counts how often it runs.
    struct Summing(Arc<AtomicUsize>);

    impl Part for Summing {
        fn run(&self, inputs: &[Input<'_>], _: &[&str]) -> Result<Vec<Tensor>, EngineError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            let sum = inputs
                .iter()
                .map(|input| match input {
                    Input::Floats { values, .. } => values.iter().sum::<f32>(),
                    Input::Ints { values, .. } => values.iter().map(|&n| f32::from(i16::try_from(n).unwrap())).sum(),
                })
                .sum();
            Ok(vec![Tensor { values: vec![sum], shape: vec![1] }])
        }
    }

    #[test]
    fn each_input_is_given_its_own_outputs_and_run_once() {
        let runs = Arc::new(AtomicUsize::new(0));
        let remembering = Remembering::new(Box::new(Summing(runs.clone())));
        let made = |features: &[f32], prompt: i32| {
            let inputs = [
                Input::Floats { name: "mel", values: features, shape: &[2] },
                Input::Ints { name: "prompt_id", values: &[prompt], shape: &[1] },
            ];
            remembering.run(&inputs, &["encoder"]).unwrap()[0].values[0]
        };

        let firsts = [made(&[1.0, 2.0], 0), made(&[3.0, 4.0], 0), made(&[1.0, 2.0], 127)];
        let agains = [made(&[1.0, 2.0], 0), made(&[3.0, 4.0], 0), made(&[1.0, 2.0], 127)];

        assert_eq!(firsts, [3.0, 7.0, 130.0], "the same features in another language are another input");
        assert_eq!(agains, firsts);
        assert_eq!(runs.load(Ordering::Relaxed), 3);
    }
}
