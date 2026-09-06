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
        // HV-21 bench hook: `iris_bench` launches this Activity only to give
        // WifiP2pManager.discoverPeers() a foreground Activity in the calling
        // (org.iris.mesh) UID — the Mobly instrumented process has none. In
        // that mode the test's snippet owns the engine, so the app must NOT
        // also auto-start its Hilt @Singleton engine (two engines fight over
        // the P2P channel + GATT server). Harmless in production: a normal
        // launch never carries this extra.
        suppressAutoStartMesh = intent?.getBooleanExtra(EXTRA_NO_AUTOSTART_MESH, false) == true
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

    companion object {
        /** Intent extra: launch foreground but let the caller's engine own the mesh. */
        const val EXTRA_NO_AUTOSTART_MESH = "iris.bench.noAutoStartMesh"

        /** Set from [EXTRA_NO_AUTOSTART_MESH] in [onCreate]; read by ConsoleScreen. */
        @Volatile
        @JvmStatic
        var suppressAutoStartMesh: Boolean = false
    }
}
