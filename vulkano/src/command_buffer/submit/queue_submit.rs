// Copyright (c) 2017 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use crate::check_errors;
use crate::command_buffer::sys::UnsafeCommandBuffer;
use crate::device::Queue;
use crate::sync::Fence;
use crate::sync::PipelineStages;
use crate::sync::Semaphore;
use crate::Error;
use crate::OomError;
use crate::SynchronizedVulkanObject;
use crate::VulkanObject;
use smallvec::SmallVec;
use std::error;
use std::fmt;
use std::marker::PhantomData;

/// Prototype for a submission that executes command buffers.
// TODO: example here
#[derive(Debug)]
pub struct SubmitCommandBufferBuilder<'a> {
    wait_semaphores: SmallVec<[ash::vk::Semaphore; 16]>,
    destination_stages: SmallVec<[ash::vk::PipelineStageFlags; 8]>,
    signal_semaphores: SmallVec<[ash::vk::Semaphore; 16]>,
    command_buffers: SmallVec<[ash::vk::CommandBuffer; 4]>,
    fence: ash::vk::Fence,
    marker: PhantomData<&'a ()>,
}

impl<'a> SubmitCommandBufferBuilder<'a> {
    /// Builds a new empty `SubmitCommandBufferBuilder`.
    #[inline]
    pub fn new() -> SubmitCommandBufferBuilder<'a> {
        SubmitCommandBufferBuilder {
            wait_semaphores: SmallVec::new(),
            destination_stages: SmallVec::new(),
            signal_semaphores: SmallVec::new(),
            command_buffers: SmallVec::new(),
            fence: ash::vk::Fence::null(),
            marker: PhantomData,
        }
    }

    /// Returns true if this builder will signal a fence when submitted.
    ///
    /// # Example
    ///
    /// ```
    /// use vulkano::command_buffer::submit::SubmitCommandBufferBuilder;
    /// use vulkano::sync::Fence;
    /// # let device: std::sync::Arc<vulkano::device::Device> = return;
    ///
    /// unsafe {
    ///     let fence = Fence::from_pool(device.clone()).unwrap();
    ///
    ///     let mut builder = SubmitCommandBufferBuilder::new();
    ///     assert!(!builder.has_fence());
    ///     builder.set_fence_signal(&fence);
    ///     assert!(builder.has_fence());
    /// }
    /// ```
    #[inline]
    pub fn has_fence(&self) -> bool {
        self.fence != ash::vk::Fence::null()
    }

    /// Adds an operation that signals a fence after this submission ends.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use vulkano::command_buffer::submit::SubmitCommandBufferBuilder;
    /// use vulkano::sync::Fence;
    /// # let device: std::sync::Arc<vulkano::device::Device> = return;
    /// # let queue: std::sync::Arc<vulkano::device::Queue> = return;
    ///
    /// unsafe {
    ///     let fence = Fence::from_pool(device.clone()).unwrap();
    ///
    ///     let mut builder = SubmitCommandBufferBuilder::new();
    ///     builder.set_fence_signal(&fence);
    ///
    ///     builder.submit(&queue).unwrap();
    ///
    ///     // We must not destroy the fence before it is signaled.
    ///     fence.wait(Some(Duration::from_secs(5))).unwrap();
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// - The fence must not be signaled at the time when you call `submit()`.
    ///
    /// - If you use the fence for multiple submissions, only one at a time must be executed by the
    ///   GPU. In other words, you must submit one, wait for the fence to be signaled, then reset
    ///   the fence, and then only submit the second.
    ///
    /// - If you submit this builder, the fence must be kept alive until it is signaled by the GPU.
    ///   Destroying the fence earlier is an undefined behavior.
    ///
    /// - The fence, command buffers, and semaphores must all belong to the same device.
    ///
    #[inline]
    pub unsafe fn set_fence_signal(&mut self, fence: &'a Fence) {
        self.fence = fence.internal_object();
    }

    /// Adds a semaphore to be waited upon before the command buffers are executed.
    ///
    /// Only the given `stages` of the command buffers added afterwards will wait upon
    /// the semaphore. Other stages not included in `stages` can execute before waiting.
    ///
    /// # Safety
    ///
    /// - The stages must be supported by the device.
    ///
    /// - If you submit this builder, the semaphore must be kept alive until you are guaranteed
    ///   that the GPU has at least started executing the command buffers.
    ///
    /// - If you submit this builder, no other queue must be waiting on these semaphores. In other
    ///   words, each semaphore signal can only correspond to one semaphore wait.
    ///
    /// - If you submit this builder, the semaphores must be signaled when the queue execution
    ///   reaches this submission, or there must be one or more submissions in queues that are
    ///   going to signal these semaphores. In other words, you must not block the queue with
    ///   semaphores that can't get signaled.
    ///
    /// - The fence, command buffers, and semaphores must all belong to the same device.
    ///
    #[inline]
    pub unsafe fn add_wait_semaphore(&mut self, semaphore: &'a Semaphore, stages: PipelineStages) {
        debug_assert!(!ash::vk::PipelineStageFlags::from(stages).is_empty());
        // TODO: debug assert that the device supports the stages
        self.wait_semaphores.push(semaphore.internal_object());
        self.destination_stages.push(stages.into());
    }

    /// Adds a command buffer that is executed as part of this command.
    ///
    /// The command buffers are submitted in the order in which they are added.
    ///
    /// # Safety
    ///
    /// - If you submit this builder, the command buffer must be kept alive until you are
    ///   guaranteed that the GPU has finished executing it.
    ///
    /// - Any calls to vkCmdSetEvent, vkCmdResetEvent or vkCmdWaitEvents that have been recorded
    ///   into the command buffer must not reference any VkEvent that is referenced by any of
    ///   those commands that is pending execution on another queue.
    ///   TODO: rephrase ^ ?
    ///
    /// - The fence, command buffers, and semaphores must all belong to the same device.
    ///
    /// TODO: more here
    ///
    #[inline]
    pub unsafe fn add_command_buffer(&mut self, command_buffer: &'a UnsafeCommandBuffer) {
        self.command_buffers.push(command_buffer.internal_object());
    }

    /// Returns the number of semaphores to signal.
    ///
    /// In other words, this is the number of times `add_signal_semaphore` has been called.
    #[inline]
    pub fn num_signal_semaphores(&self) -> usize {
        self.signal_semaphores.len()
    }

    /// Adds a semaphore that is going to be signaled at the end of the submission.
    ///
    /// # Safety
    ///
    /// - If you submit this builder, the semaphore must be kept alive until you are guaranteed
    ///   that the GPU has finished executing this submission.
    ///
    /// - The semaphore must be in the unsignaled state when queue execution reaches this
    ///   submission.
    ///
    /// - The fence, command buffers, and semaphores must all belong to the same device.
    ///
    #[inline]
    pub unsafe fn add_signal_semaphore(&mut self, semaphore: &'a Semaphore) {
        self.signal_semaphores.push(semaphore.internal_object());
    }

    /// Submits the command buffer to the given queue.
    ///
    /// > **Note**: This is an expensive operation, so you may want to merge as many builders as
    /// > possible together and avoid submitting them one by one.
    ///
    pub fn submit(self, queue: &Queue) -> Result<(), SubmitCommandBufferError> {
        unsafe {
            let fns = queue.device().fns();
            let queue = queue.internal_object_guard();

            debug_assert_eq!(self.wait_semaphores.len(), self.destination_stages.len());

            let batch = ash::vk::SubmitInfo {
                wait_semaphore_count: self.wait_semaphores.len() as u32,
                p_wait_semaphores: self.wait_semaphores.as_ptr(),
                p_wait_dst_stage_mask: self.destination_stages.as_ptr(),
                command_buffer_count: self.command_buffers.len() as u32,
                p_command_buffers: self.command_buffers.as_ptr(),
                signal_semaphore_count: self.signal_semaphores.len() as u32,
                p_signal_semaphores: self.signal_semaphores.as_ptr(),
                ..Default::default()
            };

            check_errors((fns.v1_0.queue_submit)(*queue, 1, &batch, self.fence))?;
            Ok(())
        }
    }

    /// Merges this builder with another builder.
    ///
    /// # Panic
    ///
    /// Panics if both builders have a fence already set.
    // TODO: create multiple batches instead
    pub fn merge(mut self, other: Self) -> Self {
        assert!(
            self.fence == ash::vk::Fence::null() || other.fence == ash::vk::Fence::null(),
            "Can't merge two queue submits that both have a fence"
        );

        self.wait_semaphores.extend(other.wait_semaphores);
        self.destination_stages.extend(other.destination_stages); // TODO: meh? will be solved if we submit multiple batches
        self.signal_semaphores.extend(other.signal_semaphores);
        self.command_buffers.extend(other.command_buffers);

        if self.fence == ash::vk::Fence::null() {
            self.fence = other.fence;
        }

        self
    }
}

/// Error that can happen when submitting the prototype.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SubmitCommandBufferError {
    /// Not enough memory.
    OomError(OomError),

    /// The connection to the device has been lost.
    DeviceLost,
}

impl error::Error for SubmitCommandBufferError {
    #[inline]
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            SubmitCommandBufferError::OomError(ref err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for SubmitCommandBufferError {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "{}",
            match *self {
                SubmitCommandBufferError::OomError(_) => "not enough memory",
                SubmitCommandBufferError::DeviceLost =>
                    "the connection to the device has been lost",
            }
        )
    }
}

impl From<Error> for SubmitCommandBufferError {
    #[inline]
    fn from(err: Error) -> SubmitCommandBufferError {
        match err {
            err @ Error::OutOfHostMemory => SubmitCommandBufferError::OomError(OomError::from(err)),
            err @ Error::OutOfDeviceMemory => {
                SubmitCommandBufferError::OomError(OomError::from(err))
            }
            Error::DeviceLost => SubmitCommandBufferError::DeviceLost,
            _ => panic!("unexpected error: {:?}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::Fence;
    use std::time::Duration;

    #[test]
    fn empty_submit() {
        let (device, queue) = gfx_dev_and_queue!();
        let builder = SubmitCommandBufferBuilder::new();
        builder.submit(&queue).unwrap();
    }

    #[test]
    fn signal_fence() {
        unsafe {
            let (device, queue) = gfx_dev_and_queue!();

            let fence = Fence::new(device.clone(), Default::default()).unwrap();
            assert!(!fence.ready().unwrap());

            let mut builder = SubmitCommandBufferBuilder::new();
            builder.set_fence_signal(&fence);

            builder.submit(&queue).unwrap();
            fence.wait(Some(Duration::from_secs(5))).unwrap();
            assert!(fence.ready().unwrap());
        }
    }

    #[test]
    fn has_fence() {
        unsafe {
            let (device, queue) = gfx_dev_and_queue!();

            let fence = Fence::new(device.clone(), Default::default()).unwrap();

            let mut builder = SubmitCommandBufferBuilder::new();
            assert!(!builder.has_fence());
            builder.set_fence_signal(&fence);
            assert!(builder.has_fence());
        }
    }

    #[test]
    fn merge_both_have_fences() {
        unsafe {
            let (device, _) = gfx_dev_and_queue!();

            let fence1 = Fence::new(device.clone(), Default::default()).unwrap();
            let fence2 = Fence::new(device.clone(), Default::default()).unwrap();

            let mut builder1 = SubmitCommandBufferBuilder::new();
            builder1.set_fence_signal(&fence1);
            let mut builder2 = SubmitCommandBufferBuilder::new();
            builder2.set_fence_signal(&fence2);

            assert_should_panic!("Can't merge two queue submits that both have a fence", {
                let _ = builder1.merge(builder2);
            });
        }
    }
}
