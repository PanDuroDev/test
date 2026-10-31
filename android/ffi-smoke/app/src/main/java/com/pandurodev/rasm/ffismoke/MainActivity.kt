package com.pandurodev.rasm.ffismoke

import android.app.Activity
import android.os.Bundle
import android.widget.TextView

/**
 * Minimal host for the native integration tests.
 *
 * This is not the Rasm Studio product UI.
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(
            TextView(this).apply {
                text = "Rasm Studio Rust/Kotlin FFI smoke host"
            }
        )
    }
}
