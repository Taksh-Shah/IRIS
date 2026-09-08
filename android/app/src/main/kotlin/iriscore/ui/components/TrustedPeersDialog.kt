package iriscore.ui.components

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text as MaterialText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.core.content.ContextCompat
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.ui.state.PairingUiState

private enum class PeerPanel { HOME, MY_CODE, SCAN, PASTE }

@Composable
fun TrustedPeersDialog(
    contacts: Map<String, String>,
    pairing: PairingUiState,
    myPairingCode: String?,
    onDismiss: () -> Unit,
    onLoadMyCode: () -> Unit,
    onBeginPairing: (String) -> Unit,
    onConfirmPairing: (String) -> Unit,
    onResetPairing: () -> Unit,
    onSelect: (String) -> Unit,
    onForget: (String) -> Unit,
) {
    val context = LocalContext.current
    var panel by rememberSaveable { mutableStateOf(PeerPanel.HOME) }
    var pastedCode by rememberSaveable { mutableStateOf("") }
    var alias by rememberSaveable { mutableStateOf("") }
    var safetyConfirmed by rememberSaveable { mutableStateOf(false) }
    var cameraGranted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        cameraGranted = it
        if (it) panel = PeerPanel.SCAN else panel = PeerPanel.PASTE
    }

    LaunchedEffect(pairing) {
        if (pairing is PairingUiState.Preview) {
            alias = ""
            safetyConfirmed = false
            onLoadMyCode()
        }
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            modifier = Modifier.fillMaxWidth().fillMaxHeight(0.92f),
            shape = IrisRadius.LG,
            color = IrisColors.BackgroundPrimary,
        ) {
            Column(
                Modifier.padding(IrisSpacing.LG).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(IrisSpacing.MD),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("TRUSTED PEERS", IrisType.Label.copy(color = IrisColors.AccentPrimary), Modifier.weight(1f))
                    Text("CLOSE", IrisType.Label, Modifier.clickable(onClick = onDismiss).padding(IrisSpacing.SM))
                }

                when (val state = pairing) {
                    PairingUiState.Working -> Text("Verifying signed pairing code…", IrisType.Secondary)
                    is PairingUiState.Preview -> PairingPreview(
                        state = state,
                        alias = alias,
                        safetyConfirmed = safetyConfirmed,
                        onAlias = { alias = it },
                        onSafetyConfirmed = { safetyConfirmed = it },
                        onConfirm = { onConfirmPairing(alias) },
                        onBack = onResetPairing,
                        myPairingCode = myPairingCode,
                    )
                    is PairingUiState.Saved -> {
                        Text("${state.alias} is verified and saved.", IrisType.Body.copy(color = IrisColors.AccentSuccess))
                        Text("Tap the contact to send. For two-way chat, scan the other phone in the opposite direction too.", IrisType.Secondary)
                        Button(onClick = {
                            onSelect(state.peerIdHex)
                            onDismiss()
                        }) { MaterialText("Send to ${state.alias}") }
                    }
                    is PairingUiState.Error -> {
                        Text(state.message, IrisType.Body.copy(color = IrisColors.AccentCritical))
                        Button(onClick = onResetPairing) { MaterialText("Try again") }
                    }
                    PairingUiState.Idle -> when (panel) {
                        PeerPanel.HOME -> PeerHome(
                            contacts = contacts,
                            onMyCode = {
                                panel = PeerPanel.MY_CODE
                                onLoadMyCode()
                            },
                            onScan = {
                                if (cameraGranted) panel = PeerPanel.SCAN else cameraPermission.launch(Manifest.permission.CAMERA)
                            },
                            onPaste = { panel = PeerPanel.PASTE },
                            onSelect = {
                                onSelect(it)
                                onDismiss()
                            },
                            onForget = onForget,
                        )
                        PeerPanel.MY_CODE -> {
                            Text("Let your friend scan this code", IrisType.Body)
                            Text("It contains only your signed public identity and encryption key.", IrisType.Secondary)
                            if (myPairingCode == null) {
                                Text("Preparing code…", IrisType.Secondary)
                            } else {
                                PairingQr(myPairingCode, Modifier.align(Alignment.CenterHorizontally).size(280.dp))
                                Button(onClick = {
                                    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    clipboard.setPrimaryClip(ClipData.newPlainText("IRIS pairing code", myPairingCode))
                                }) { MaterialText("Copy code") }
                            }
                            Button(onClick = { panel = PeerPanel.HOME }) { MaterialText("Back") }
                        }
                        PeerPanel.SCAN -> {
                            Text("Scan your friend's IRIS code", IrisType.Body)
                            PairingScanner(
                                onCode = onBeginPairing,
                                onError = { panel = PeerPanel.PASTE },
                            )
                            Text("Keep both phones close and compare the safety code next.", IrisType.Secondary)
                            Button(onClick = { panel = PeerPanel.PASTE }) { MaterialText("Paste instead") }
                        }
                        PeerPanel.PASTE -> {
                            Text("Paste the full IRIS pairing code", IrisType.Body)
                            Text("Use this when your friend is far away. Do not enter separate loose keys.", IrisType.Secondary)
                            OutlinedTextField(
                                value = pastedCode,
                                onValueChange = { if (it.length <= 512) pastedCode = it },
                                modifier = Modifier.fillMaxWidth(),
                                minLines = 4,
                                label = { MaterialText("iris:pair:v1:…") },
                            )
                            Button(enabled = pastedCode.isNotBlank(), onClick = { onBeginPairing(pastedCode) }) {
                                MaterialText("Verify code")
                            }
                            Button(onClick = { panel = PeerPanel.HOME }) { MaterialText("Back") }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PeerHome(
    contacts: Map<String, String>,
    onMyCode: () -> Unit,
    onScan: () -> Unit,
    onPaste: () -> Unit,
    onSelect: (String) -> Unit,
    onForget: (String) -> Unit,
) {
    Text("Pair once, then send by name every day.", IrisType.Secondary)
    Row(horizontalArrangement = Arrangement.spacedBy(IrisSpacing.SM)) {
        Button(onClick = onScan) { MaterialText("Scan QR") }
        Button(onClick = onPaste) { MaterialText("Paste") }
    }
    Button(onClick = onMyCode) { MaterialText("Show my code") }
    Text("SAVED", IrisType.Label.copy(color = IrisColors.TextTertiary))
    if (contacts.isEmpty()) {
        Text("No verified contacts yet.", IrisType.Secondary)
    } else {
        contacts.entries.sortedBy { it.value.lowercase() }.forEach { (peerId, name) ->
            Row(
                Modifier.fillMaxWidth().background(IrisColors.SurfacePrimary, IrisRadius.MD).padding(IrisSpacing.MD),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f).clickable { onSelect(peerId) }) {
                    Text(name, IrisType.Body)
                    Text(peerId.take(12) + "…", IrisType.Meta)
                }
                Text("FORGET", IrisType.Label.copy(color = IrisColors.AccentCritical), Modifier.clickable { onForget(peerId) }.padding(IrisSpacing.SM))
            }
        }
    }
}

@Composable
private fun PairingPreview(
    state: PairingUiState.Preview,
    alias: String,
    safetyConfirmed: Boolean,
    onAlias: (String) -> Unit,
    onSafetyConfirmed: (Boolean) -> Unit,
    onConfirm: () -> Unit,
    onBack: () -> Unit,
    myPairingCode: String?,
) {
    Text("COMPARE ON BOTH PHONES", IrisType.Label.copy(color = IrisColors.AccentWarning))
    Text(state.sas, IrisType.Title.copy(color = IrisColors.AccentPrimary))
    Text("Confirm only if your friend sees exactly the same safety code.", IrisType.Secondary)
    if (myPairingCode != null) {
        Text("Now let your friend scan your code", IrisType.Body)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
            PairingQr(myPairingCode, Modifier.size(180.dp))
        }
        Text("After they scan, both phones will show the same safety code.", IrisType.Secondary)
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked = safetyConfirmed, onCheckedChange = onSafetyConfirmed)
        Text("Both phones show this code", IrisType.Body, Modifier.clickable { onSafetyConfirmed(!safetyConfirmed) })
    }
    OutlinedTextField(
        value = alias,
        onValueChange = { if (it.codePointCount(0, it.length) <= 32) onAlias(it) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        label = { MaterialText("Name, e.g. Rahul") },
    )
    Text("Identity ${state.peerIdHex.take(16)}…", IrisType.Meta)
    Button(enabled = safetyConfirmed && alias.isNotBlank(), onClick = onConfirm) { MaterialText("Save verified contact") }
    Button(onClick = onBack) { MaterialText("Cancel") }
}
