//! Cantara's frontend depends on npm packages which this build skript will automatically install if the 'dist/' folder does not exist in the repository.

use std::fs;
use std::process::Command;

fn main() {
    // Check if "dist" folder exists
    if fs::metadata("node_modules").is_err() {
        // Run npm install
        let output = Command::new("npm")
            .arg("install")
            .output()
            .expect("Failed to execute npm install. Make sure that you have npm installed.");

        // Print output for debugging
        if !output.status.success() {
            eprintln!(
                "npm install failed: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
            panic!("npm install failed");
        }
    }
}
