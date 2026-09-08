package iriscore.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisType
import iriscore.ui.MeshViewModel

/**
 * WP5 — minimal settings destination.
 *
 * Shows the node identity (id + backend) and provides two actions:
 * stop the mesh transport and navigate to the pairing flow.
 */
@Composable
fun SettingsScreen(
    onNavigateToPairing: () -> Unit,
    onBack: () -> Unit,
    viewModel: MeshViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(IrisColors.BackgroundPrimary)
            .statusBarsPadding()
            .navigationBarsPadding()
            .padding(horizontal = 24.dp, vertical = 16.dp),
    ) {
        Text(
            text = "SETTINGS",
            style = IrisType.Label,
            color = IrisColors.TextSecondary,
        )

        Spacer(modifier = Modifier.height(24.dp))

        Text(
            text = "Node ID",
            style = IrisType.Meta,
            color = IrisColors.TextSecondary,
        )
        Text(
            text = state.nodeIdHex,
            style = IrisType.SystemLine,
            color = IrisColors.TextPrimary,
            modifier = Modifier.padding(top = 4.dp),
        )

        Spacer(modifier = Modifier.height(16.dp))

        Text(
            text = "Identity backend",
            style = IrisType.Meta,
            color = IrisColors.TextSecondary,
        )
        Text(
            text = state.identityBackend,
            style = IrisType.SystemLine,
            color = IrisColors.TextPrimary,
            modifier = Modifier.padding(top = 4.dp),
        )

        Spacer(modifier = Modifier.height(32.dp))

        Button(
            onClick = onNavigateToPairing,
            modifier = Modifier.fillMaxWidth(),
            colors = ButtonDefaults.buttonColors(
                containerColor = IrisColors.AccentPrimary,
                contentColor = IrisColors.BackgroundPrimary,
            ),
        ) {
            Text(text = "Pair with a peer")
        }

        Spacer(modifier = Modifier.height(12.dp))

        OutlinedButton(
            onClick = { viewModel.stopMesh(); onBack() },
            modifier = Modifier.fillMaxWidth(),
            colors = ButtonDefaults.outlinedButtonColors(
                contentColor = IrisColors.AccentCritical,
            ),
        ) {
            Text(text = "Stop Mesh")
        }
    }
}
