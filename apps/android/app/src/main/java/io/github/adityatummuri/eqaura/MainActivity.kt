package io.github.adityatummuri.eqaura

import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import io.github.adityatummuri.eqaura.ui.AppRoot
import io.github.adityatummuri.eqaura.ui.ProvideEqTheme
import io.github.adityatummuri.eqaura.ui.isDark
import io.github.adityatummuri.eqaura.ui.observe

class MainActivity : ComponentActivity() {
    private lateinit var model: AppModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        model = (application as EqApp).model
        enableEdgeToEdge()
        setContent {
            val theme by model.observe { it.settings.theme }
            val dark = isDark(theme)
            // The system bar icons follow the chosen theme, not only the phone's.
            DisposableEffect(dark) {
                this@MainActivity.enableEdgeToEdge(
                    statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                    navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                )
                onDispose {}
            }
            ProvideEqTheme(dark) { AppRoot(model) }
        }
    }

    override fun onStart() {
        super.onStart()
        model.appShown()
    }

    override fun onStop() {
        model.saveNow()
        if (model.state.value.hearing.stage == HearingStage.Testing) model.cancelHearing()
        super.onStop()
    }
}
