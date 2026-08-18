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
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
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
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    enabled = recipient.isNotBlank() && text.isNotBlank(),
                    onClick = {
                        viewModel.send(recipient, text, 0u)
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
                    key = { "${it.senderId}:${it.receivedAtMs}:${it.pending}" },
                ) { message ->
                    MessageRow(message)
                }
            }
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