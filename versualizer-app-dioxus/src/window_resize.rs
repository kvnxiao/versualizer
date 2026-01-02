//! Window auto-resize functionality for CSS-driven sizing.
//!
//! This module provides a hook that automatically resizes the OS window
//! when CSS changes affect the content dimensions.

use dioxus::desktop::{LogicalSize, use_window};
use dioxus::prelude::*;
use serde::Deserialize;
use tracing::{debug, warn};

/// Dimensions returned from JavaScript measurement.
#[derive(Debug, Deserialize)]
struct MeasuredDimensions {
    width: Option<f64>,
    height: Option<f64>,
}

/// Hook that auto-resizes the window when CSS changes.
///
/// This hook monitors a CSS content signal and, after each change,
/// measures the `.lines` element dimensions and resizes the window accordingly.
///
/// # Arguments
///
/// * `css_signal` - A signal containing the current CSS content (triggers resize on change)
pub fn use_window_auto_resize(css_signal: Signal<String>) {
    let window = use_window();

    // Track CSS changes and trigger measurement
    use_effect(move || {
        // Read CSS to establish reactive dependency
        let _ = css_signal.read();

        let window = window.clone();

        spawn(async move {
            // Wait for next animation frame to ensure CSS is applied
            // Using double-RAF for reliability across browsers
            let js_measure = r#"
                return new Promise((resolve) => {
                    requestAnimationFrame(() => {
                        requestAnimationFrame(() => {
                            const lines = document.querySelector('.lines');
                            const root = document.documentElement;
                            const computedStyle = getComputedStyle(root);

                            // Check for explicit CSS variable overrides
                            const cssWidth = computedStyle.getPropertyValue('--window-width').trim();
                            const cssHeight = computedStyle.getPropertyValue('--window-height').trim();

                            let width = null;
                            let height = null;

                            // Parse CSS variable if specified (e.g., "800px" -> 800)
                            if (cssWidth && cssWidth !== '') {
                                const parsed = parseFloat(cssWidth);
                                if (!isNaN(parsed)) width = parsed;
                            }

                            if (cssHeight && cssHeight !== '') {
                                const parsed = parseFloat(cssHeight);
                                if (!isNaN(parsed)) height = parsed;
                            }

                            // If no explicit height, measure .lines element
                            if (height === null && lines) {
                                const rect = lines.getBoundingClientRect();
                                height = rect.height;
                            }

                            // If no explicit width, measure .app element
                            if (width === null) {
                                const app = document.querySelector('.app');
                                if (app) {
                                    const rect = app.getBoundingClientRect();
                                    width = rect.width;
                                }
                            }

                            resolve({ width, height });
                        });
                    });
                });
            "#;

            // Execute JavaScript to measure dimensions
            let eval_result = document::eval(js_measure);

            match eval_result.await {
                Ok(value) => match serde_json::from_value::<MeasuredDimensions>(value) {
                    Ok(dims) => {
                        // Get current window size for fallback
                        let current_size = window.inner_size();

                        // Use measured/CSS values or fallback to current
                        let current_width = f64::from(current_size.width);
                        let current_height = f64::from(current_size.height);

                        let new_width = dims.width.unwrap_or(current_width);
                        let new_height = dims.height.unwrap_or(current_height);

                        // Only resize if dimensions actually changed (threshold of 1px)
                        let width_changed = (new_width - current_width).abs() > 1.0;
                        let height_changed = (new_height - current_height).abs() > 1.0;

                        if width_changed || height_changed {
                            debug!(
                                "Auto-resizing window: {}x{} -> {}x{}",
                                current_width, current_height, new_width, new_height
                            );

                            window.set_inner_size(LogicalSize::new(new_width, new_height));
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse measured dimensions: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to measure window dimensions: {}", e);
                }
            }
        });
    });
}
