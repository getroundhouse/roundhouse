//! A model part run on Core ML, through objc2's bindings, with no Swift. Each run copies its inputs into new
//! `MLMultiArray`s, of 32-bit floats or integers, as every part of the pinned export takes, and reads the outputs asked
//! for by their strides, since the encoders' rows are padded. Making the arrays anew each run cost nothing measurable,
//! against keeping them.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::ptr::NonNull;

use block2::StackBlock;
use objc2::AnyThread as _;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel, MLModelConfiguration,
    MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSInteger, NSNumber, NSString, NSURL};

use super::parts::{Input, Part, Tensor};
use crate::EngineError;

/// A part, loaded by Core ML. Core ML's models aren't `Send`, so it's used on the thread that loaded it.
///
/// That thread is the engine's own, which has no autorelease pool, so each call into Core ML is made in one of its own,
/// and so is letting go of the model: otherwise what Core ML autoreleases, about an object a prediction, is never freed.
pub(super) struct CoreMlPart {
    /// Let go of in a pool, by `drop`.
    model: ManuallyDrop<Retained<MLModel>>,
}

impl Drop for CoreMlPart {
    fn drop(&mut self) {
        // SAFETY: `drop` runs once, and nothing uses the model after it.
        let model = unsafe { ManuallyDrop::take(&mut self.model) };
        autoreleasepool(|_| drop(model));
    }
}

impl CoreMlPart {
    /// The compiled encoder at `path`, a `.mlmodelc` directory, to run on the Neural Engine.
    pub(super) fn load(path: &Path) -> Result<Self, EngineError> {
        autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            // SAFETY: a new configuration, set before anything else has it.
            let configuration = unsafe {
                let configuration = MLModelConfiguration::new();
                configuration.setComputeUnits(MLComputeUnits::CPUAndNeuralEngine);
                configuration
            };
            // SAFETY: loads from a file URL, with a configuration it copies, and reports what fails as an error.
            let model = unsafe { MLModel::modelWithContentsOfURL_configuration_error(&url, &configuration) }
                .map_err(|e| failed(&format!("Core ML couldn't load {}", path.display()), &e))?;
            Ok(Self { model: ManuallyDrop::new(model) })
        })
    }
}

impl Part for CoreMlPart {
    fn run(&self, inputs: &[Input<'_>], outputs: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        autoreleasepool(|_| {
            let provider = provider(inputs)?;
            // SAFETY: a prediction on the thread that loaded the model, from inputs it checks against its description.
            let features = unsafe { self.model.predictionFromFeatures_error(ProtocolObject::from_ref(&*provider)) }
                .map_err(|e| failed("Core ML couldn't run the model", &e))?;
            outputs.iter().map(|name| read(&features, name)).collect()
        })
    }
}

/// The inputs, by name, as Core ML takes them.
fn provider(inputs: &[Input<'_>]) -> Result<Retained<MLDictionaryFeatureProvider>, EngineError> {
    let mut names = Vec::new();
    let mut values: Vec<Retained<AnyObject>> = Vec::new();
    for input in inputs {
        let (Input::Floats { name, .. } | Input::Ints { name, .. }) = input;
        names.push(NSString::from_str(name));
        let array = array(input)?;
        // SAFETY: wraps an array that's kept alive by the value it's put in.
        let value = unsafe { MLFeatureValue::featureValueWithMultiArray(&array) };
        values.push(Retained::into_super(Retained::into_super(value)));
    }
    let names: Vec<&NSString> = names.iter().map(|name| &**name).collect();
    let dictionary: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_retained_objects(&names, &values);
    // SAFETY: a dictionary of feature values by name, as the initialiser takes, which it checks.
    unsafe { MLDictionaryFeatureProvider::initWithDictionary_error(MLDictionaryFeatureProvider::alloc(), &dictionary) }
        .map_err(|e| failed("Core ML couldn't take the inputs", &e))
}

/// A new array holding `input`'s values: 32-bit floats or integers, as the input has them, and as every part of the
/// pinned export declares. What Core ML does with a part that declares another type is untried.
fn array(input: &Input<'_>) -> Result<Retained<MLMultiArray>, EngineError> {
    let (name, shape, kind, count) = match input {
        Input::Floats { name, values, shape } => (*name, *shape, MLMultiArrayDataType::Float32, values.len()),
        Input::Ints { name, values, shape } => (*name, *shape, MLMultiArrayDataType::Int32, values.len()),
    };
    if count != shape.iter().product::<usize>() {
        return Err(EngineError(format!("{name} has {count} values for a shape of {shape:?}")));
    }
    let dimensions: Vec<Retained<NSNumber>> = shape.iter().map(|&d| NSNumber::new_usize(d)).collect();
    let dimensions = NSArray::from_retained_slice(&dimensions);
    // SAFETY: a new array of a shape and type Core ML checks, which reports what it can't make as an error.
    let array = unsafe { MLMultiArray::initWithShape_dataType_error(MLMultiArray::alloc(), &dimensions, kind) }
        .map_err(|e| failed(&format!("Core ML couldn't make {name}"), &e))?;
    let written = RefCell::new(Err(EngineError(format!("Core ML gave nowhere to write {name}"))));
    let write = StackBlock::new(|bytes: NonNull<c_void>, size: NSInteger, strides: NonNull<NSArray<NSNumber>>| {
        // SAFETY: the strides of the buffer Core ML hands this block, which it keeps until the block returns.
        let strides = numbers(unsafe { strides.as_ref() });
        let buffer = Buffer { start: bytes, bytes: size, shape, strides: &strides };
        // SAFETY: the buffer is the array's, of `kind`, for this block alone, until it returns.
        *written.borrow_mut() = unsafe {
            match input {
                Input::Floats { values, .. } => buffer.write(values, name),
                Input::Ints { values, .. } => buffer.write(values, name),
            }
        };
    });
    // SAFETY: calls the block at once, with the array's buffer, and doesn't keep it.
    unsafe { array.getMutableBytesWithHandler(&write) };
    written.into_inner()?;
    Ok(array)
}

/// The output `name` of `features`, its values first dimension first, whatever its strides.
fn read(features: &ProtocolObject<dyn MLFeatureProvider>, name: &str) -> Result<Tensor, EngineError> {
    let missing = || EngineError(format!("the model gave no {name}"));
    // SAFETY: reads an output of a finished prediction, which nothing else holds.
    let value = unsafe { features.featureValueForName(&NSString::from_str(name)) }.ok_or_else(missing)?;
    // SAFETY: as above.
    let array = unsafe { value.multiArrayValue() }.ok_or_else(missing)?;
    // SAFETY: as above.
    let (shape, strides, kind) = unsafe { (numbers(&array.shape()), numbers(&array.strides()), array.dataType()) };
    let read = RefCell::new(Err(EngineError(format!("Core ML gave nothing to read of {name}"))));
    let block = StackBlock::new(|bytes: NonNull<c_void>, size: NSInteger| {
        let buffer = Buffer { start: bytes, bytes: size, shape: &shape, strides: &strides };
        // SAFETY: the buffer is the array's, of `kind`, until the block returns.
        *read.borrow_mut() = unsafe {
            match kind {
                MLMultiArrayDataType::Float32 => buffer.read::<f32>(name),
                MLMultiArrayDataType::Int32 => {
                    buffer.read::<i32>(name).map(|ints| ints.into_iter().map(float).collect())
                }
                other => {
                    Err(EngineError(format!("{name} is {other:?}, where 32-bit floats or integers were expected")))
                }
            }
        };
    });
    // SAFETY: calls the block at once, with the array's buffer, and doesn't keep it.
    unsafe { array.getBytesWithHandler(&block) };
    Ok(Tensor { values: read.into_inner()?, shape })
}

/// An array's buffer, as Core ML hands it to a block, and how its elements are laid out in it.
struct Buffer<'a> {
    start: NonNull<c_void>,
    /// How long it is, in bytes.
    bytes: NSInteger,
    shape: &'a [usize],
    /// How many elements apart the next of each dimension is.
    strides: &'a [usize],
}

impl Buffer<'_> {
    /// Each element's offset, in elements, first dimension first, once each is checked to be in the buffer for
    /// elements of `size` bytes.
    fn offsets(&self, size: usize, name: &str) -> Result<Vec<usize>, EngineError> {
        if self.strides.len() != self.shape.len() {
            return Err(EngineError(format!(
                "{name} has {} strides for {} dimensions",
                self.strides.len(),
                self.shape.len()
            )));
        }
        let past = || EngineError(format!("{name}'s elements reach past its buffer of {} bytes", self.bytes));
        // Checked, since a shape and strides that overflow would otherwise wrap round to an offset that looks in.
        let count = self.shape.iter().try_fold(1_usize, |count, &n| count.checked_mul(n)).ok_or_else(past)?;
        let last = self.shape.iter().zip(self.strides).try_fold(0_usize, |last, (n, stride)| {
            n.saturating_sub(1).checked_mul(*stride).and_then(|reach| last.checked_add(reach))
        });
        let room = usize::try_from(self.bytes).unwrap_or(0) / size;
        if count > 0 && last.is_none_or(|last| last >= room) {
            return Err(past());
        }
        let mut offsets = Vec::with_capacity(count);
        let mut index = vec![0; self.shape.len()];
        for _ in 0..count {
            offsets.push(index.iter().zip(self.strides).map(|(i, stride)| i * stride).sum());
            // The next element, the last dimension first.
            for (dimension, i) in index.iter_mut().enumerate().rev() {
                *i += 1;
                if *i < self.shape[dimension] {
                    break;
                }
                *i = 0;
            }
        }
        Ok(offsets)
    }

    /// Copies `values` in, first dimension first.
    ///
    /// # Safety
    ///
    /// The buffer holds elements of `T`, and nothing else reads or writes it meanwhile.
    unsafe fn write<T: Copy>(&self, values: &[T], name: &str) -> Result<(), EngineError> {
        let start = self.start.as_ptr().cast::<T>();
        for (offset, value) in self.offsets(size_of::<T>(), name)?.into_iter().zip(values) {
            // SAFETY: `offset` is within the buffer, as `offsets` checked, for elements of `T`, as the caller says.
            unsafe { start.add(offset).write(*value) };
        }
        Ok(())
    }

    /// Its values, first dimension first.
    ///
    /// # Safety
    ///
    /// The buffer holds elements of `T`, and nothing writes it meanwhile.
    unsafe fn read<T: Copy>(&self, name: &str) -> Result<Vec<T>, EngineError> {
        let start = self.start.as_ptr().cast::<T>();
        let offsets = self.offsets(size_of::<T>(), name)?;
        // SAFETY: each offset is within the buffer, as `offsets` checked, for elements of `T`, as the caller says.
        Ok(offsets.into_iter().map(|offset| unsafe { start.add(offset).read() }).collect())
    }
}

fn numbers(numbers: &NSArray<NSNumber>) -> Vec<usize> {
    numbers.iter().map(|n| n.as_usize()).collect()
}

fn failed(what: &str, error: &NSError) -> EngineError {
    EngineError(format!("{what}: {}", error.localizedDescription()))
}

#[expect(clippy::cast_precision_loss, reason = "a count or a token's number, far below 2^24")]
fn float(value: i32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use super::Buffer;

    #[test]
    fn a_buffer_with_padded_rows_reads_only_its_elements_and_one_too_short_is_refused() {
        // Two rows of three, each padded to four.
        let mut held = [1.0_f32, 2.0, 3.0, -1.0, 4.0, 5.0, 6.0, -1.0];
        let start = NonNull::new(held.as_mut_ptr().cast::<c_void>()).unwrap();
        let bytes = isize::try_from(size_of_val(&held)).unwrap();
        let buffer = Buffer { start, bytes, shape: &[2, 3], strides: &[4, 1] };
        // SAFETY: `held` holds f32s, and nothing else touches it.
        assert_eq!(unsafe { buffer.read::<f32>("held") }.unwrap(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        let short = Buffer { bytes: bytes - 8, ..buffer };
        // SAFETY: as above.
        assert!(unsafe { short.read::<f32>("held") }.is_err(), "its last element would be past the end");

        // A stride that wraps round, twice over, to an offset of 0, if it weren't checked.
        let wrapping = Buffer { shape: &[3, 2], strides: &[usize::MAX / 2 + 1, 1], ..buffer };
        // SAFETY: as above.
        assert!(unsafe { wrapping.read::<f32>("held") }.is_err(), "its strides reach far past the end");
    }
}
