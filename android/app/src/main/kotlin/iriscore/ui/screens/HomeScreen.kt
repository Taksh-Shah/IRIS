package iriscore.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.R
import iriscore.ui.MeshViewModel
import iriscore.ui.state.InboxUiMessage
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.util.MeshPermissions

/**
 * Default text tier — P4, matching `ContentType::Text.assign_priority()` in
 * protocol/content_type.rs so the shell agrees with the core's own mapping.
 */
private const val PRIORITY_NORMAL: UByte = 4u

/** P0 — the no-drop SOS tier (`ContentType::Sos`). */
private const val PRIORITY_SOS: UByte = 0u

/**
 * AC-6 — home screen. Consumes [MeshUiState] via
 * `collectAsStateWithLifecycle()` (lifecycle-aware, strong-skipping safe) and
 * exposes a minimal send/relay surface for the P0 demo path.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(viewModel: MeshViewModel = hiltViewModel()) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()

    var recipient by rememberSaveable { mutableStateOf("") }
    var text by rememberSaveable { mutableStateOf("") }

    val context = LocalContext.current
    var permissionsGranted by rememberSaveable {
        mutableStateOf(MeshPermissions.allGranted(context))
    }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        // Re-read from the system rather than trusting the result map: a
        // permission already granted before this request is absent from it.
        permissionsGranted = MeshPermissions.allGranted(context)
    }

    // Ask once on first composition; bring the transports up as soon as the
    // grants are in place (the ViewModel call is idempotent).
    LaunchedEffect(permissionsGranted) {
        if (permissionsGranted) {
            viewModel.ensureStarted()
        } else {
            val missing = MeshPermissions.missing(context)
            if (missing.isNotEmpty()) permissionLauncher.launch(missing.toTypedArray())
        }
    }

    Scaffold(
        topBar = { TopAppBar(title = { Text(stringResource(R.string.home_title)) }) },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .padding(16.dp)
                .fillMaxSize(),
        ) {
            NodeStatusCard(state)
            if (!permissionsGranted) {
                Spacer(Modifier.height(12.dp))
                PermissionCard(
                    onGrant = {
                        val missing = MeshPermissions.missing(context)
                        if (missing.isNotEmpty()) permissionLauncher.launch(missing.toTypedArray())
                    },
                )
            }
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = recipient,
                onValueChange = { recipient = it },
                label = { Text(stringResource(R.string.recipient_label)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text(stringResource(R.string.message_hint)) },
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            state.lastError?.let { error ->
                Spacer(Modifier.height(8.dp))
                Text(
                    text = error,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            Spacer(Modifier.height(8.dp))
            val canSend = recipient.isNotBlank() && text.isNotBlank()
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                // Normal priority is the default path; P0 is the no-drop SOS
                // tier and must stay an explicit choice — sending everything at
                // P0 (the previous behaviour) collapses the priority system.
                Button(
                    enabled = canSend,
                    onClick = {
                        viewModel.send(recipient, text, PRIORITY_NORMAL)
                        text = ""
                    },
                ) { Text(stringResource(R.string.send_action)) }
                Button(
                    enabled = canSend,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = MaterialTheme.colorScheme.onError,
                    ),
                    onClick = {
                        viewModel.send(recipient, text, PRIORITY_SOS)
                        text = ""
                    },
                ) { Text(stringResource(R.string.send_p0_action)) }
                Button(onClick = { viewModel.syncRelayNow() }) { Text(stringResource(R.string.relay_action)) }
            }
            Spacer(Modifier.height(16.dp))
            LazyColumn(
                modifier = Modifier.weight(1f),
            ) {
                items(
                    items = state.messages,
                    key = { it.uid },
                ) { message ->
                    MessageRow(message)
                }
            }
        }
    }
}

/**
 * Shown while any runtime grant is outstanding. Without these the radios accept
 * calls but return nothing, so the mesh would look healthy and carry no traffic
 * — the user needs to see that, not a silently dead node.
 */
@Composable
private fun PermissionCard(onGrant: () -> Unit) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = stringResource(R.string.permissions_title),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.error,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                text = stringResource(R.string.permissions_body),
                style = MaterialTheme.typography.bodySmall,
            )
            Spacer(Modifier.height(8.dp))
            Button(onClick = onGrant) { Text(stringResource(R.string.permissions_action)) }
        }
    }
}

@Composable
private fun NodeStatusCard(state: MeshUiState) {
    val statusText = stringResource(
        when (state.status) {
            MeshStatus.IDLE -> R.string.status_idle
            MeshStatus.STARTING -> R.string.status_starting
            MeshStatus.RUNNING -> R.string.status_running
            MeshStatus.UNAVAILABLE -> R.string.status_unavailable
        },
    )
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = "${stringResource(R.string.node_id_label)} ${state.nodeIdShort}",
                style = MaterialTheme.typography.titleMedium,
                fontFamily = FontFamily.Monospace,
            )
            Text(
                text = "identity: ${state.identityBackend} · status: $statusText · relay: ${state.relayQueued}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun MessageRow(message: InboxUiMessage) {
    Card(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        Column(Modifier.padding(12.dp)) {
            Text(
                text = "${message.senderId.take(8)}… · P${message.priority}",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
                fontFamily = FontFamily.Monospace,
            )
            Text(
                text = message.payloadUtf8,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}