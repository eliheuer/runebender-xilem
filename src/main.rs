// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Runebender Xilem: A font editor built with Xilem

use xilem::EventLoop;
use xilem::winit::error::EventLoopError;

fn main() -> Result<(), EventLoopError> {
    runebender::run(EventLoop::with_user_event())
}
