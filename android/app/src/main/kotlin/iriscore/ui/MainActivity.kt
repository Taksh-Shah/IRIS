package iriscore.ui

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import dagger.hilt.android.AndroidEntryPoint
import iriscore.designsystem.IrisColors
import iriscore.ui.screens.ConsoleScreen

@AndroidEntryPoint
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        // Edge-to-edge with transparent bars: the console paints true black to
        // the physical edges, and the screen applies its own inset padding.
        // Both styles are pinned dark because IRIS has no light theme — the
        // deep-black environment is the design, not a preference.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(IrisColors.BackgroundPrimary.toArgb()),
            navigationBarStyle = SystemBarStyle.dark(IrisColors.BackgroundPrimary.toArgb()),
        )
        super.onCreate(savedInstanceState)
        setContent {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(IrisColors.BackgroundPrimary),
            ) {
                ConsoleScreen()
            }
        }
    }
}
