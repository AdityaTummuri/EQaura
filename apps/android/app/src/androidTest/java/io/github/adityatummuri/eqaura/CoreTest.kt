package io.github.adityatummuri.eqaura

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/** The Rust core answers on a real device, in the shapes the app expects. */
@RunWith(AndroidJUnit4::class)
class CoreTest {
    private val profile = Profile(filters = listOf(Filter(Kind.LowShelf, 105.0, 4.0, 0.7)))

    @Test
    fun analysis() {
        val a = Core.analyze(profile, 0)
        assertEquals(Core.POINTS, a.left.size)
        assertNull(a.right)
        assertNotNull(a.selected)
        assertTrue(a.preampDb < -3.9 && a.netPeakDb < 0.01)
    }

    @Test
    fun everyCallWorks() {
        assertTrue(Core.builtin().size >= 10)
        assertEquals(1200.0, Core.number("1.2k")!!, 1e-9)
        assertNull(Core.number("loud"))
        assertEquals(6.0, Core.slider(emptyList(), 3, 6.0).single().gainDb, 1e-9)
        val hits = Core.search("hd600", 5)
        val layer = Core.headphone(hits.rows.first().index)
        assertTrue(layer.filters.isNotEmpty())
        val full = profile.copy(headphone = layer)
        assertNotNull(Core.analyze(full, null).correction)
        assertEquals(full, Core.json.decodeFromString<Profile>(Core.json.encodeToString(full)))
        val back = Core.import("Back", Core.export(profile))
        assertEquals(1, back.profile.filters.size)
        assertThrows(CoreError::class.java) { Core.import("x", "hello") }
        assertTrue(Core.sends(profile, "Speaker", false).contains("Speaker"))
        assertNotNull(Core.suggest("more bass", profile))
        assertNull(Core.suggest("qwertz", profile))
        // Refused before anything is sent: there is no key.
        val target = AiTarget("openai", "", "x", null)
        assertThrows(CoreError::class.java) { Core.ask(target, "a", "b", false, emptyList(), profile) }
        assertTrue(Core.tone(true, 1000.0, -40.0, 48_000.0).isNotEmpty())
    }

    @Test
    fun hearingTestEnds() {
        var answers = emptyList<Boolean>()
        repeat(1000) { i ->
            val step = Core.hearing(answers)
            if (step.done) {
                assertEquals(8, step.bars.size)
                assertEquals(8, Core.earBars(step.layer!!).size)
                return
            }
            answers = answers + (i % 3 != 0)
        }
        fail("the test never ended")
    }

    @Test
    fun numbersReadWell() {
        assertEquals("63 Hz", freqText(63.0))
        assertEquals("1.25 kHz", freqText(1250.0))
        assertEquals("−3.5 dB", gainText(-3.5))
        assertEquals("0 dB", gainText(-0.01))
        assertEquals(1230.0, roundFreq(1234.0), 1e-9)
    }
}
