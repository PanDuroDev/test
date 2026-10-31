package com.pandurodev.rasm.ffismoke

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.pandurodev.rasm.ffi.StrokeSampleInput
import com.pandurodev.rasm.ffi.bridgeVersion
import com.pandurodev.rasm.ffi.previewStroke
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RustBridgeInstrumentedTest {
    @Test
    fun generatedBindingsLoadTheAndroidNativeLibrary() {
        assertEquals("rasm-engine-ffi/1", bridgeVersion())
    }

    @Test
    fun validStrokeCrossesTheBoundaryAndReturnsBoundsAndTiles() {
        val preview = previewStroke(
            listOf(
                StrokeSampleInput(
                    x = -20.0,
                    y = 2.0,
                    pressure = 1.0f,
                    timestampNs = 0L
                ),
                StrokeSampleInput(
                    x = 20.0,
                    y = 2.0,
                    pressure = 1.0f,
                    timestampNs = 1_000_000L
                )
            ),
            tileSize = 64,
            margin = 2.0
        )

        assertTrue(preview.bounds.minX < -27.0)
        assertTrue(preview.bounds.maxX > 27.0)
        assertTrue(preview.tiles.isNotEmpty())
        assertTrue(preview.tiles.size <= 16_384)
    }

    @Test
    fun invalidTimestampIsReturnedAsAnFfiFailure() {
        val failure = runCatching {
            previewStroke(
                listOf(
                    StrokeSampleInput(
                        x = 0.0,
                        y = 0.0,
                        pressure = 1.0f,
                        timestampNs = -1L
                    )
                ),
                tileSize = 64,
                margin = 0.0
            )
        }.exceptionOrNull()

        assertNotNull("Expected the Rust boundary to reject a negative timestamp", failure)
        assertTrue(
            "Expected the Rust validation message, got: $failure",
            failure?.message.orEmpty().contains("timestamp", ignoreCase = true)
        )
        assertTrue("Native library did not load", failure !is UnsatisfiedLinkError)
    }
}
