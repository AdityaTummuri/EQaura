package io.github.adityatummuri.eqaura

import android.Manifest
import android.app.NotificationManager
import android.graphics.Bitmap
import android.os.Build
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Drives the whole app the way a person would. */
@RunWith(AndroidJUnit4::class)
class AppTest {
    @get:Rule(order = 0)
    val permission: GrantPermissionRule = if (Build.VERSION.SDK_INT >= 33) {
        GrantPermissionRule.grant(Manifest.permission.POST_NOTIFICATIONS)
    } else {
        GrantPermissionRule.grant()
    }

    @get:Rule(order = 1)
    val ui = createAndroidComposeRule<MainActivity>()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val model get() = (instrumentation.targetContext.applicationContext as EqApp).model
    private val state get() = model.state.value

    private fun main(f: AppModel.() -> Unit) {
        ui.runOnUiThread { model.f() }
        ui.waitForIdle()
    }

    private fun tap(tag: String) {
        val node = ui.onNodeWithTag(tag)
        try {
            node.performScrollTo()
        } catch (_: AssertionError) {
            // Not inside anything that scrolls.
        }
        node.performClick()
        ui.waitForIdle()
    }

    private fun back() {
        ui.activityRule.scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
        ui.waitForIdle()
    }

    private fun await(what: String, condition: () -> Boolean) = ui.waitUntil(what, 10_000, condition)

    private fun shoot(name: String) {
        val dir = InstrumentationRegistry.getArguments().getString("shots") ?: return
        ui.waitForIdle()
        Thread.sleep(700)
        val shot = instrumentation.uiAutomation.takeScreenshot() ?: return
        File(dir).mkdirs()
        File(dir, "$name.png").outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun reset() = main {
        setEnabled(false)
        cancelHearing()
        clearChat()
        removeHeadphone()
        removeHearing()
        setAutoPreamp(true)
        clearBands()
        setSliderView(false)
        setCoverage(Coverage.AllApps)
        setDetail(Detail.Balanced)
        setAi(AiChoice.Rules)
        setTheme(ThemeChoice.Light)
        state.value.presets.filter { !it.builtin }.forEach { deletePreset(it.name) }
        setWelcomed()
    }

    @Before
    fun start() = reset()

    @After
    fun stop() = reset()

    @Test
    fun powerStartsAndStopsTheService() {
        tap("nav-equalizer")
        tap("power")
        await("active") { state.effects == EffectState.Active }
        val notifications = ui.activity.getSystemService(NotificationManager::class.java)
        await("notification") { notifications.activeNotifications.isNotEmpty() }
        tap("power")
        await("off") { state.effects == EffectState.Off }
        await("no notification") { notifications.activeNotifications.isEmpty() }
    }

    @Test
    fun bandsCanBeEditedAndUndone() {
        tap("nav-equalizer")
        ui.onNodeWithTag("graph").assertIsDisplayed()
        tap("add-band")
        assertEquals(1, state.profile.filters.size)
        assertEquals(0, state.selected)
        main {
            dragBand(0, 1234.0, 5.04)
            dragBand(0, 2468.0, 6.04)
            seal()
        }
        assertEquals(Filter(Kind.Peak, 2470.0, 6.0, 1.0), state.profile.filters[0])
        await("curve") { state.analysis.left.any { it > 5.5f } }
        tap("undo")
        assertEquals(0.0, state.profile.filters[0].gainDb, 0.0)
        tap("redo")
        assertEquals(6.0, state.profile.filters[0].gainDb, 0.0)
        shoot("band-light")
        tap("compare")
        assertTrue(state.comparing)
        tap("compare")
        assertFalse(state.comparing)
        main { select(0) }
        tap("delete-band")
        assertTrue(state.profile.filters.isEmpty())
    }

    @Test
    fun slidersAndPresets() {
        tap("nav-equalizer")
        tap("view-sliders")
        ui.onNodeWithTag("slider-0").assertExists()
        ui.onNodeWithTag("slider-9").assertExists()
        main {
            setSlider(2, 4f)
            seal()
        }
        await("slider") { state.analysis.sliders[2] == 4f }
        shoot("sliders-light")
        main { assertTrue(savePreset("My sound")) }
        assertEquals("My sound", state.preset)
        assertFalse(state.dirty)
        main { loadPreset("Bass lift") }
        assertEquals("Bass lift", state.preset)
        tap("preset-chip")
        shoot("presets-light")
        back()
        main {
            assertFalse(savePreset("Flat"))
            assertTrue(renamePreset("My sound", "Mine"))
            loadPreset("Mine")
            deletePreset("Mine")
        }
        assertNull(state.preset)
        val text = model.exportText()
        main {
            clearBands()
            importText("Back", text)
        }
        assertEquals(4.0, state.profile.filters.single().gainDb, 0.0)
    }

    @Test
    fun headphones() {
        tap("nav-headphones")
        ui.onNodeWithTag("headphone-search").performTextInput("hd 600")
        // The search waits for a pause in typing, then runs on another thread.
        ui.mainClock.advanceTimeBy(1000)
        Thread.sleep(1500)
        ui.waitUntil("results", 10_000) {
            ui.onAllNodes(hasTestTag("headphone-row-0")).fetchSemanticsNodes().isNotEmpty()
        }
        ui.waitForIdle()
        tap("headphone-row-0")
        await("picked") { state.profile.headphone?.name?.contains("600") == true }
        shoot("headphones-light")
    }

    @Test
    fun hearing() {
        tap("nav-hearing")
        tap("hearing-start")
        assertEquals(HearingStage.Testing, state.hearing.stage)
        shoot("hearing-light")
        tap("hearing-yes")
        tap("hearing-no")
        var i = 0
        while (state.hearing.stage == HearingStage.Testing && i < 1000) {
            // Pretend to hear worse at the top of the left ear.
            val step = state.hearing.step!!
            val limit = if (step.left && step.freq > 3000) -45.0 else -70.0
            main { answerHearing(step.db > limit) }
            i++
        }
        assertEquals(HearingStage.Result, state.hearing.stage)
        shoot("hearing-result-light")
        tap("hearing-apply")
        val layer = state.profile.hearing
        assertNotNull(layer)
        assertTrue(layer!!.left.isNotEmpty())
        await("bars") { state.hearing.bars.size == 8 }
        shoot("hearing-applied-light")
    }

    @Test
    fun assistant() {
        tap("nav-assistant")
        ui.onNodeWithTag("assistant-input").performTextInput("more bass")
        tap("assistant-send")
        await("answer") { state.chat.size == 2 }
        tap("proposal-apply")
        assertTrue(state.profile.filters.isNotEmpty())
        assertEquals(ProposalState.Applied, state.chat[1].proposal)
        main { setTheme(ThemeChoice.Dark) }
        shoot("assistant-dark")
        main { undo() }
        assertEquals(ProposalState.Undone, state.chat[1].proposal)
        main { send("qwertz") }
        assertEquals(4, state.chat.size)
    }

    @Test
    fun everyPageOpensInBothThemes() {
        main { loadPreset("Warm") }
        for (theme in listOf(ThemeChoice.Light, ThemeChoice.Dark)) {
            main { setTheme(theme) }
            val name = theme.name.lowercase()
            for (page in listOf("equalizer", "headphones", "hearing", "assistant")) {
                tap("nav-$page")
                shoot("page-$page-$name")
            }
            for (page in listOf("settings", "system", "about")) {
                tap("nav-more")
                tap("more-$page")
                shoot("page-$page-$name")
                back()
            }
        }
    }

    @Test
    fun stateSurvivesRecreation() {
        tap("nav-equalizer")
        main { addBand(440.0, 3.0) }
        ui.activityRule.scenario.recreate()
        ui.waitForIdle()
        assertEquals(440.0, state.profile.filters[0].freqHz, 0.0)
        ui.onNodeWithTag("graph").assertIsDisplayed()
    }

    @Test
    fun keysAreStoredEncrypted() {
        val secret = "test-key-123"
        main {
            setAi(AiChoice.OpenAi)
            setAiKey(secret)
        }
        assertTrue(state.ai.hasKey)
        assertEquals(secret, Keys(instrumentation.targetContext).get("openai"))
        main { saveNow() }
        Thread.sleep(500)
        File(instrumentation.targetContext.applicationInfo.dataDir).walk()
            .filter { it.isFile && it.extension in setOf("xml", "json") }
            .forEach { assertFalse(it.name, it.readText().contains(secret)) }
        main { setAiKey("") }
        assertFalse(state.ai.hasKey)
    }

    @Test
    fun settingsAreRemembered() {
        main {
            setDetail(Detail.FineBass)
            addBand(100.0, 2.0)
            saveNow()
        }
        Thread.sleep(500)
        val again = Store(instrumentation.targetContext.filesDir)
        assertEquals(Detail.FineBass, again.settings().detail)
        assertEquals(2.0, again.devices().getValue(state.output.id).profile.filters[0].gainDb, 0.0)
    }
}
