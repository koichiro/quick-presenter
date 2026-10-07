#![cfg(target_os = "linux")]

use slint::winit_030::winit;

// Exercise the exact vendored implementation through the application's test harness.
include!("../vendor/i-slint-backend-winit/startup_notify.rs");
