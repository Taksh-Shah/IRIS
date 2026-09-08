package iriscore.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.TabRowDefaults.tabIndicatorOffset
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisType
import iriscore.ui.MeshViewModel
import iriscore.ui.components.PairingQr
import iriscore.ui.components.PairingScanner
import iriscore.ui.state.PairingUiState

/**
 * WP10 — dedicated pairing screen.
 *
 * Two tabs: "Show my code" (PairingQr) and "Scan peer code" (PairingScanner).
 * Wires up MeshViewModel.beginPairing / confirmPairing / resetPairing and
 * displays the full PairingUiState machine.
 */
@Composable
fun PairingScreen(
    onBack: () -> Unit,
    viewModel: MeshViewModel = hiltViewModel(),
) {
    val pairing by viewModel.pairing.collectAsStateWithLifecycle()
    val myCode by viewModel.myPairingCode.collectAsStateWithLifecycle()
    var selectedTab by rememberSaveable { mutableIntStateOf(0) }
    var alias by rememberSaveable { mutableStateOf("") }

    LaunchedEffect(Unit) {
        viewModel.loadMyPairingCode()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(IrisColors.BackgroundPrimary)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        // --- Header ---
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            TextButton(onClick = onBack) {
                Text(
                    text = "Back",
                    style = IrisType.Body,
                    color = IrisColors.AccentPrimary,
                )
            }
            Spacer(modifier = Modifier.width(8.dp))
            Text(
                text = "PAIRING",
                style = IrisType.Label,
                color = IrisColors.TextSecondary,
            )
        }

        // --- Tabs ---
        TabRow(
            selectedTabIndex = selectedTab,
            containerColor = IrisColors.SurfacePrimary,
            contentColor = IrisColors.TextPrimary,
            indicator = { tabPositions ->
                TabRowDefaults.SecondaryIndicator(
                    modifier = Modifier.tabIndicatorOffset(tabPositions[selectedTab]),
                    color = IrisColors.AccentPrimary,
                )
            },
        ) {
            Tab(
                selected = selectedTab == 0,
                onClick = { selectedTab = 0 },
                text = {
                    Text(
                        text = "Show my code",
                        style = IrisType.Meta,
                        color = if (selectedTab == 0) IrisColors.TextPrimary else IrisColors.TextSecondary,
                    )
                },
            )
            Tab(
                selected = selectedTab == 1,
                onClick = { selectedTab = 1 },
                text = {
                    Text(
                        text = "Scan peer code",
                        style = IrisType.Meta,
                        color = if (selectedTab == 1) IrisColors.TextPrimary else IrisColors.TextSecondary,
                    )
                },
            )
        }

        // --- Tab content ---
        when (selectedTab) {
            0 -> ShowMyCodeTab(myCode = myCode)
            1 -> ScanPeerCodeTab(
                pairing = pairing,
                alias = alias,
                onAliasChange = { alias = it },
                onCodeScanned = { code ->
                    viewModel.beginPairing(code)
                    alias = ""
                },
                onConfirm = { viewModel.confirmPairing(alias) },
                onReset = {
                    viewModel.resetPairing()
                    alias = ""
                },
            )
        }
    }
}

@Composable
private fun ShowMyCodeTab(myCode: String?) {
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .fillMaxWidth()
            .padding(32.dp),
    ) {
        if (myCode != null) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                PairingQr(
                    code = myCode,
                    modifier = Modifier.size(280.dp),
                )
                Spacer(modifier = Modifier.height(16.dp))
                Text(
                    text = "Show this QR to a peer to pair",
                    style = IrisType.Meta,
                    color = IrisColors.TextSecondary,
                )
            }
        } else {
            CircularProgressIndicator(color = IrisColors.AccentPrimary)
        }
    }
}

@Composable
private fun ScanPeerCodeTab(
    pairing: PairingUiState,
    alias: String,
    onAliasChange: (String) -> Unit,
    onCodeScanned: (String) -> Unit,
    onConfirm: () -> Unit,
    onReset: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        when (pairing) {
            is PairingUiState.Idle -> {
                Text(
                    text = "Point the camera at a peer's QR code",
                    style = IrisType.Meta,
                    color = IrisColors.TextSecondary,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
                PairingScanner(
                    onCode = onCodeScanned,
                    onError = { /* error surfaced by camera permissions */ },
                )
            }

            is PairingUiState.Working -> {
                Spacer(modifier = Modifier.height(40.dp))
                CircularProgressIndicator(color = IrisColors.AccentPrimary)
                Spacer(modifier = Modifier.height(16.dp))
                Text(
                    text = "Verifying pairing code…",
                    style = IrisType.Meta,
                    color = IrisColors.TextSecondary,
                )
            }

            is PairingUiState.Preview -> {
                Text(
                    text = "Verification code",
                    style = IrisType.Label,
                    color = IrisColors.TextSecondary,
                )
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = pairing.sas,
                    style = IrisType.SystemLine,
                    color = IrisColors.AccentPrimary,
                )
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = "Confirm this matches the peer's screen",
                    style = IrisType.Meta,
                    color = IrisColors.TextSecondary,
                )
                Spacer(modifier = Modifier.height(24.dp))
                TextField(
                    value = alias,
                    onValueChange = onAliasChange,
                    label = {
                        Text(
                            text = "Contact name",
                            style = IrisType.Meta,
                            color = IrisColors.TextSecondary,
                        )
                    },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    colors = TextFieldDefaults.colors(
                        focusedContainerColor = IrisColors.SurfacePrimary,
                        unfocusedContainerColor = IrisColors.SurfacePrimary,
                        focusedTextColor = IrisColors.TextPrimary,
                        unfocusedTextColor = IrisColors.TextPrimary,
                        focusedIndicatorColor = IrisColors.AccentPrimary,
                        unfocusedIndicatorColor = IrisColors.BorderSubtle,
                        cursorColor = IrisColors.AccentPrimary,
                    ),
                )
                Spacer(modifier = Modifier.height(16.dp))
                Button(
                    onClick = onConfirm,
                    enabled = alias.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = IrisColors.AccentPrimary,
                        contentColor = IrisColors.BackgroundPrimary,
                    ),
                ) {
                    Text(text = "Verify and save contact")
                }
                Spacer(modifier = Modifier.height(8.dp))
                TextButton(onClick = onReset, modifier = Modifier.fillMaxWidth()) {
                    Text(
                        text = "Cancel",
                        color = IrisColors.TextSecondary,
                    )
                }
            }

            is PairingUiState.Saved -> {
                Spacer(modifier = Modifier.height(24.dp))
                Text(
                    text = "Paired with ${pairing.alias}",
                    style = IrisType.Body,
                    color = IrisColors.AccentSuccess,
                )
                Spacer(modifier = Modifier.height(16.dp))
                TextButton(onClick = onReset, modifier = Modifier.fillMaxWidth()) {
                    Text(
                        text = "Pair another peer",
                        color = IrisColors.AccentPrimary,
                    )
                }
            }

            is PairingUiState.Error -> {
                Spacer(modifier = Modifier.height(24.dp))
                Text(
                    text = pairing.message,
                    style = IrisType.Body,
                    color = IrisColors.AccentCritical,
                )
                Spacer(modifier = Modifier.height(16.dp))
                Button(
                    onClick = onReset,
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = IrisColors.SurfacePrimary,
                        contentColor = IrisColors.TextPrimary,
                    ),
                ) {
                    Text(text = "Try again")
                }
            }
        }
    }
}
