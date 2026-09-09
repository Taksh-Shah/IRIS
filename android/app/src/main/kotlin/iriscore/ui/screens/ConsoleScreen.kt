@file:OptIn(ExperimentalMaterial3Api::class)

package iriscore.ui.screens

import android.app.Activity
import android.bluetooth.BluetoothManager
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.app.ActivityCompat
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.command.CommandRegistry
import iriscore.command.ConsoleInputParser
import iriscore.command.InputMode
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisSizing
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.designsystem.glass.ProvideGlassTier
import iriscore.designsystem.glass.glassBackdrop
import iriscore.ui.MeshViewModel
import iriscore.ui.components.IrisCommandPalette
import iriscore.ui.components.IrisConsoleInput
import iriscore.ui.components.IrisMessage
import iriscore.ui.components.IrisStatusChip
import iriscore.ui.components.IrisSystemEvent
import iriscore.ui.components.StatusTone
import iriscore.ui.components.Text
import iriscore.ui.components.TrustedPeersDialog
import iriscore.ui.state.ConsoleEntry
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.ui.state.TransportStatus
import iriscore.ui.state.plainLanguage
import iriscore.util.MeshPermissions
import kotlinx.coroutines.launch

/**
 * The IRIS console — the single screen of the mobile shell.
 *
 * Structure is deliberately flat: a quiet status line, the transcript, and one
 * floating input. Commands, system output and human messages share the stream,
 * so running a command never feels like leaving the conversation.
 */
@Composable
fun ConsoleScreen(viewModel: MeshViewModel = hiltViewModel()) = ProvideGlassTier {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    val entries by viewModel.console.collectAsStateWithLifecycle()
    val recipient by viewModel.recipient.collectAsStateWithLifecycle()
    // HV-59: per-transport chip data from the 5-s ViewModel poll.
    val transportStates by viewModel.transportStates.collectAsStateWithLifecycle()
    // HV-56: contact book for name resolution throughout the screen.
    val contacts by viewModel.contacts.collectAsStateWithLifecycle()
    // HV-60: retry counter drives escalated help text in RetryNotice.
    val reconnectAttempts by viewModel.reconnectAttempts.collectAsStateWithLifecycle()
    val pairing by viewModel.pairing.collectAsStateWithLifecycle()
    val myPairingCode by viewModel.myPairingCode.collectAsStateWithLifecycle()

    var input by rememberSaveable { mutableStateOf("") }
    var selectedCommand by remember { mutableStateOf(0) }
    var peersOpen by rememberSaveable { mutableStateOf(false) }
    var transportSheetOpen by rememberSaveable { mutableStateOf(false) }

    // WP12: which message (if any) has its long-press action sheet or info
    // panel open. Not rememberSaveable — an InboxUiMessage isn't a stable
    // saved-state type, and re-showing a dialog across a config change/
    // process restart for a transient interaction isn't worth carrying.
    var actionSheetTarget by remember { mutableStateOf<iriscore.ui.state.InboxUiMessage?>(null) }
    var infoDialogTarget by remember { mutableStateOf<iriscore.ui.state.InboxUiMessage?>(null) }
    val clipboardContext = LocalContext.current

    actionSheetTarget?.let { target ->
        iriscore.ui.components.MessageActionSheet(
            message = target,
            onDismiss = { actionSheetTarget = null },
            onReply = if (!target.isOutbound) {
                { viewModel.replyTo(target.senderId) }
            } else {
                null
            },
            onRetry = if (target.isOutbound &&
                (
                    target.deliveryStatus == iriscore.ui.state.DeliveryStatus.FAILED ||
                        target.deliveryStatus == iriscore.ui.state.DeliveryStatus.EXPIRED
                    )
            ) {
                { viewModel.retryMessage(target.uid) }
            } else {
                null
            },
            onCopied = {
                android.widget.Toast.makeText(
                    clipboardContext,
                    clipboardContext.getString(iriscore.R.string.msg_copied),
                    android.widget.Toast.LENGTH_SHORT,
                ).show()
            },
            onShowInfo = { infoDialogTarget = target },
            onDelete = {
                viewModel.deleteMessage(target.uid)
                android.widget.Toast.makeText(
                    clipboardContext,
                    clipboardContext.getString(iriscore.R.string.msg_deleted),
                    android.widget.Toast.LENGTH_SHORT,
                ).show()
            },
        )
    }
    infoDialogTarget?.let { target ->
        iriscore.ui.components.MessageInfoDialog(
            message = target,
            // senderId holds the peer either way (sender for inbound,
            // recipient for outbound) — the same contact lookup applies.
            fromLabel = contacts[target.senderId] ?: target.senderId.take(16),
            onDismiss = { infoDialogTarget = null },
        )
    }

    if (transportSheetOpen) {
        TransportSheet(
            transportStates = transportStates,
            meshRunning = state.status == MeshStatus.RUNNING,
            onDismiss = { transportSheetOpen = false },
        )
    }

    if (peersOpen) {
        TrustedPeersDialog(
            contacts = contacts,
            pairing = pairing,
            myPairingCode = myPairingCode,
            onDismiss = {
                peersOpen = false
                viewModel.resetPairing()
            },
            onLoadMyCode = viewModel::loadMyPairingCode,
            onBeginPairing = viewModel::beginPairing,
            onConfirmPairing = viewModel::confirmPairing,
            onResetPairing = viewModel::resetPairing,
            onSelect = viewModel::selectContact,
            onForget = viewModel::forgetContact,
        )
    }

    val mode = remember(input) { ConsoleInputParser.parse(input) }
    val matches = remember(mode) {
        (mode as? InputMode.Command)?.let { CommandRegistry.search(it.token) }.orEmpty()
    }
    val paletteVisible = mode is InputMode.Command && matches.isNotEmpty()

    // Keep the selection in range as the filtered set shrinks under the cursor.
    LaunchedEffect(matches) { selectedCommand = selectedCommand.coerceIn(0, maxOf(0, matches.size - 1)) }

    val context = LocalContext.current
    val activity = context as? Activity
    // HV-61: track whether we have ever issued the permission request so we can
    // distinguish "never asked" (shouldShowRationale == false, first launch) from
    // "permanently denied" (shouldShowRationale == false AFTER at least one ask).
    var hasRequestedPermissions by rememberSaveable { mutableStateOf(false) }
    var permissionsGranted by rememberSaveable { mutableStateOf(MeshPermissions.allGranted(context)) }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        // Re-read from the system: permissions already held before the request
        // are absent from the result map.
        permissionsGranted = MeshPermissions.allGranted(context)
    }

    // HV-61: "don't ask again" detection. After one denial, shouldShowRationale
    // flips to true (can ask again); after the user selects "Don't ask again" it
    // flips back to false. So permanently-denied = asked at least once AND every
    // missing permission now returns false from shouldShowRationale.
    val permanentlyDenied = hasRequestedPermissions && !permissionsGranted &&
        MeshPermissions.missing(context).none { perm ->
            activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, perm)
        }

    // HV-30: a runtime grant revoked from Settings while the app is backgrounded
    // is otherwise never noticed — the composition's cached `permissionsGranted`
    // stays true and the UI shows RUNNING against dark radios. Re-read on every
    // resume; the core has already demoted the transport to Unavailable, so the
    // RetryNotice fires and this surfaces the PermissionNotice to re-grant.
    LifecycleResumeEffect(Unit) {
        permissionsGranted = MeshPermissions.allGranted(context)
        onPauseOrDispose { }
    }

    LaunchedEffect(permissionsGranted) {
        if (permissionsGranted && !iriscore.ui.MainActivity.suppressAutoStartMesh) {
            viewModel.ensureStarted()
        } else if (permissionsGranted) {
            // HV-21 bench: the Activity is up only to satisfy the OEM
            // "app has a foreground Activity" gate for WifiP2pManager
            // .discoverPeers(); the test's own engine owns the mesh.
        } else {
            MeshPermissions.missing(context)
                .takeIf { it.isNotEmpty() }
                ?.let {
                    hasRequestedPermissions = true
                    permissionLauncher.launch(it.toTypedArray())
                }
        }
    }

    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()

    // HV-63: only auto-scroll when the user is already near the bottom — within
    // 2 items of the last entry. If they've scrolled up to read history, let them:
    // new messages are counted and shown in the "N new ↓" pill instead.
    val atBottom by remember {
        derivedStateOf {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: return@derivedStateOf true
            last >= info.totalItemsCount - 3
        }
    }
    var unseenCount by remember { mutableIntStateOf(0) }

    // Scroll to tail when a new entry arrives — but only if already at the bottom.
    LaunchedEffect(entries.size) {
        if (entries.isNotEmpty()) {
            if (atBottom) {
                listState.animateScrollToItem(entries.lastIndex)
            } else {
                unseenCount++
            }
        }
    }
    // Reset the unseen badge whenever the user scrolls back to the bottom.
    LaunchedEffect(atBottom) {
        if (atBottom) unseenCount = 0
    }

    // HV-54: the composer floats over the transcript and grows upward as the
    // user types a multi-line message (up to 5 lines), but the list used to
    // reserve a fixed bottom padding sized for a one-line composer — so a
    // longer message covered the newest 1-3 messages with no way to scroll
    // them into view (the list believed it was already at the end). Measure
    // the actual floating column's height and feed it into the list's
    // bottom padding so the reserved space always matches what's really
    // floating above it.
    val density = LocalDensity.current
    var floatingHeight by remember { mutableStateOf(IrisSizing.InputHeight + IrisSpacing.XXL) }
    // Re-follow the tail as the composer grows/shrinks too, not just when a
    // new entry arrives — otherwise the last message can still slide back
    // under the composer as it expands without the list re-scrolling.
    LaunchedEffect(floatingHeight) {
        if (entries.isNotEmpty() && atBottom) listState.animateScrollToItem(entries.lastIndex)
    }

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(IrisColors.BackgroundPrimary)
            // Consume BOTH the navigation-bar and IME insets at the root so the
            // entire layout (message list + floating input) stays above the
            // on-screen keyboard AND above the navigation bar. Handling them
            // together avoids the double-padding gap that appears on devices
            // where the nav bar remains visible alongside the keyboard.
            .navigationBarsPadding()
            .imePadding(),
    ) {
        // Phase 6: the layout adapts by width rather than assuming a phone.
        // Past the tablet breakpoint the reading column is capped and centred
        // so message text does not run to 120+ characters a line, and the
        // gutters open up. Chrome still spans the full window.
        val wide = maxWidth >= IrisSizing.TabletBreakpoint
        val gutter = if (wide) IrisSizing.WideGutter else IrisSpacing.Gutter
        val contentWidth: Modifier =
            if (wide) {
                Modifier.widthIn(max = IrisSizing.ReadableContentWidth)
            } else {
                Modifier.fillMaxWidth()
            }

        Column(
            Modifier
                .fillMaxSize()
                // The transcript recedes while the palette is open — this is the
                // backdrop half of the glass effect.
                .glassBackdrop(active = paletteVisible),
        ) {
            StatusLine(
                state = state,
                recipient = recipient,
                contacts = contacts,
                permissionsGranted = permissionsGranted,
                transportStates = transportStates,
                onOpenPeers = { peersOpen = true },
                onOpenTransports = { transportSheetOpen = true },
                modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars),
            )

            if (!permissionsGranted) {
                PermissionNotice(
                    permanentlyDenied = permanentlyDenied,
                    onGrant = {
                        if (permanentlyDenied) {
                            // HV-61: permanently denied — take the user to Settings.
                            context.startActivity(
                                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                                    data = Uri.fromParts("package", context.packageName, null)
                                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                                },
                            )
                        } else {
                            MeshPermissions.missing(context)
                                .takeIf { it.isNotEmpty() }
                                ?.let {
                                    hasRequestedPermissions = true
                                    permissionLauncher.launch(it.toTypedArray())
                                }
                        }
                    },
                )
            }
            if (state.status == MeshStatus.UNAVAILABLE ||
                (state.status == MeshStatus.STARTING && reconnectAttempts > 0)
            ) {
                RetryNotice(
                    isRetrying = state.status == MeshStatus.STARTING,
                    reconnectAttempts = reconnectAttempts,
                    onRetry = { viewModel.reconnectMesh() },
                )
            }

            LazyColumn(
                state = listState,
                modifier = Modifier
                    .weight(1f)
                    .align(Alignment.CenterHorizontally)
                    .then(contentWidth),
                // The composer floats OVER this list, so the transcript has to
                // reserve room for it. Without this the newest entry — the one
                // the user most wants to read — sits underneath the input and
                // cannot be scrolled into view, because the list believes it is
                // already at the end.
                contentPadding = PaddingValues(
                    start = gutter,
                    end = gutter,
                    bottom = floatingHeight + IrisSpacing.SM,
                ),
            ) {
                items(count = entries.size, key = { entries[it].uid }) { index ->
                    ConsoleRow(
                        entries = entries,
                        index = index,
                        contacts = contacts,
                        onReply = { senderId -> viewModel.replyTo(senderId) },
                        onLongPress = { message -> actionSheetTarget = message },
                    )
                }
            }
        }

        // Input and palette float above the transcript, pinned to the bottom.
        // A vertical gradient fades from transparent at the top to the app
        // background at the bottom so the last message is never obscured by
        // the input bar (the same pattern used by WhatsApp / Messenger).
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .then(contentWidth)
                .background(
                    Brush.verticalGradient(
                        0f to Color.Transparent,
                        0.25f to IrisColors.BackgroundPrimary.copy(alpha = 0.92f),
                        1f to IrisColors.BackgroundPrimary,
                    ),
                )
                .padding(horizontal = IrisSpacing.MD, vertical = IrisSpacing.MD)
                .onGloballyPositioned { coords ->
                    val measured = with(density) { coords.size.height.toDp() }
                    if (measured.value > 0f) floatingHeight = measured
                },
        ) {
            // HV-63: when the user has scrolled up and new messages arrive, show
            // a pill instead of force-scrolling them back to the bottom.
            if (unseenCount > 0) {
                Row(
                    Modifier
                        .align(Alignment.CenterHorizontally)
                        .clickable {
                            scope.launch {
                                if (entries.isNotEmpty()) {
                                    listState.animateScrollToItem(entries.lastIndex)
                                }
                                unseenCount = 0
                            }
                        }
                        .background(IrisColors.AccentPrimary.copy(alpha = 0.92f), iriscore.designsystem.IrisRadius.Full)
                        .padding(horizontal = IrisSpacing.MD, vertical = IrisSpacing.XS),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(IrisSpacing.XS),
                ) {
                    Text(
                        text = "$unseenCount new  ↓",
                        style = IrisType.Label.copy(color = IrisColors.BackgroundPrimary),
                    )
                }
                Spacer(Modifier.height(IrisSpacing.SM))
            }

            IrisCommandPalette(
                visible = paletteVisible,
                commands = matches,
                selectedIndex = selectedCommand,
                onSelect = { command ->
                    // Completing a command leaves the cursor ready for its
                    // argument rather than running a command that needs one.
                    input = if (command.argumentHint != null) "${command.invocation} " else command.invocation
                },
            )
            if (paletteVisible) Spacer(Modifier.height(IrisSpacing.SM))

            // Failures raised by the engine itself (rejected send, bad
            // recipient) surface here; command-level errors already appear in
            // the transcript as system events.
            state.lastError?.let { error ->
                Text(
                    text = error,
                    style = IrisType.SystemLine.copy(color = IrisColors.AccentCritical),
                    modifier = Modifier.padding(
                        start = IrisSpacing.SM,
                        end = IrisSpacing.SM,
                        bottom = IrisSpacing.SM,
                    ),
                )
            }

            IrisConsoleInput(
                value = input,
                mode = mode,
                onValueChange = { input = it },
                onSubmit = {
                    viewModel.submit(input)
                    input = ""
                },
            )
        }
    }
}

/**
 * One console row. Message grouping needs the previous entry, so the whole list
 * is passed rather than a single item.
 */
@Composable
private fun ConsoleRow(
    entries: List<ConsoleEntry>,
    index: Int,
    contacts: Map<String, String>,
    onReply: (String) -> Unit,
    onLongPress: (iriscore.ui.state.InboxUiMessage) -> Unit,
) {
    when (val entry = entries[index]) {
        is ConsoleEntry.Message -> {
            val previous = entries.getOrNull(index - 1)
            // Group only when the previous row has the same sender AND the same
            // direction — inbound from peer A and outbound to peer A must not merge.
            val grouped = previous is ConsoleEntry.Message &&
                previous.message.senderId == entry.message.senderId &&
                previous.message.isOutbound == entry.message.isOutbound
            IrisMessage(
                message = entry.message,
                grouped = grouped,
                // Reply is only meaningful for inbound messages.
                onReply = if (!entry.message.isOutbound) onReply else null,
                contactName = if (!entry.message.isOutbound) contacts[entry.message.senderId] else null,
                onLongPress = onLongPress,
            )
        }

        is ConsoleEntry.System -> IrisSystemEvent(
            title = entry.title,
            lines = entry.lines,
            status = entry.status,
        )

        is ConsoleEntry.Echo -> Text(
            text = entry.text,
            style = IrisType.Command.copy(color = IrisColors.TextTertiary),
            modifier = Modifier.padding(top = IrisSpacing.MD),
        )
    }
}

/**
 * The persistent system line: node identity, transport state, relay depth.
 *
 * Always present, never loud — this is the ambient state readout that makes
 * IRIS feel like an operating layer rather than a chat window.
 */
@Composable
private fun StatusLine(
    state: MeshUiState,
    recipient: String?,
    contacts: Map<String, String>,
    permissionsGranted: Boolean,
    transportStates: List<TransportStatus>,
    onOpenPeers: () -> Unit,
    onOpenTransports: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(IrisSpacing.SM),
    ) {
        Text(text = "IRIS", style = IrisType.Label.copy(color = IrisColors.TextSecondary))
        Text(text = state.nodeIdShort, style = IrisType.Meta)

        Spacer(Modifier.weight(1f))

        Text(
            text = "PEERS",
            style = IrisType.Label.copy(color = IrisColors.AccentPrimary),
            modifier = Modifier.clickable(onClick = onOpenPeers).padding(IrisSpacing.XS),
        )

        if (recipient != null) {
            Text(
                text = "→ ${contacts[recipient] ?: (recipient.take(8) + "…")}",
                style = IrisType.Meta,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (state.relayQueued > 0) {
            IrisStatusChip(
                label = "Q${state.relayQueued}",
                tone = StatusTone.Warning,
                semanticDescription = stringResource(iriscore.R.string.status_queued_fmt, state.relayQueued),
            )
        }
        // HV-59: smart transport button — ⋯ when nothing is up, otherwise the
        // active medium(s). Tapping always opens the full transport sheet.
        TransportMediaButton(
            transportStates = transportStates,
            meshRunning = state.status == MeshStatus.RUNNING,
            onClick = onOpenTransports,
        )
        // HV-61: override the global status chip with "PERM" when permissions are
        // missing so the user immediately sees why the mesh is not carrying traffic
        // instead of a misleading IDLE/RUNNING chip.
        IrisStatusChip(
            label = when {
                !permissionsGranted -> "PERM"
                state.status == MeshStatus.RUNNING -> "LINK"
                state.status == MeshStatus.STARTING -> "INIT"
                state.status == MeshStatus.IDLE -> "IDLE"
                else -> "DOWN"
            },
            tone = when {
                !permissionsGranted -> StatusTone.Critical
                state.status == MeshStatus.RUNNING -> StatusTone.Active
                state.status == MeshStatus.STARTING -> StatusTone.Warning
                state.status == MeshStatus.IDLE -> StatusTone.Neutral
                else -> StatusTone.Critical
            },
            // WP11: TalkBack hears the plain-language meaning, not the raw
            // "LINK"/"INIT"/"DOWN" abbreviation the compact on-screen chip
            // needs for width. permissionsGranted is not part of MeshStatus,
            // so it's handled as its own case here rather than in
            // MeshStatus.plainLanguage().
            semanticDescription = if (!permissionsGranted) {
                stringResource(iriscore.R.string.status_permission_needed)
            } else {
                state.status.plainLanguage()
            },
        )
    }
}

@Composable
private fun PermissionNotice(permanentlyDenied: Boolean, onGrant: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.SM)
            .background(IrisColors.SurfacePrimary, iriscore.designsystem.IrisRadius.MD)
            .clickable(onClick = onGrant)
            .padding(IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = "PERMISSIONS",
                style = IrisType.Label.copy(color = IrisColors.AccentWarning),
            )
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                // HV-61: when permanently denied, explain that the system dialog
                // will no longer appear and the user must open Settings manually.
                text = if (permanentlyDenied) {
                    "Nearby-device access permanently denied. Open Settings to re-enable."
                } else {
                    "Nearby-device access is required; without it the mesh carries no traffic."
                },
                style = IrisType.Secondary,
            )
        }
        Spacer(Modifier.width(IrisSpacing.MD))
        Text(
            text = if (permanentlyDenied) "SETTINGS" else "GRANT",
            style = IrisType.Label.copy(color = IrisColors.AccentPrimary),
        )
    }
}

@Composable
private fun RetryNotice(isRetrying: Boolean, reconnectAttempts: Int, onRetry: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.SM)
            .background(IrisColors.SurfacePrimary, iriscore.designsystem.IrisRadius.MD)
            .clickable(enabled = !isRetrying, onClick = onRetry)
            .padding(IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = if (isRetrying) "RECONNECTING…" else "MESH UNAVAILABLE",
                style = IrisType.Label.copy(
                    color = if (isRetrying) IrisColors.AccentWarning else IrisColors.AccentCritical,
                ),
            )
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                text = when {
                    isRetrying -> "Connecting to mesh — this may take a moment."
                    reconnectAttempts >= 2 -> "Still failing. Try toggling Bluetooth/Wi-Fi or restarting the app."
                    else -> "Tap to retry mesh connection."
                },
                style = IrisType.Secondary,
            )
        }
        if (!isRetrying) {
            Spacer(Modifier.width(IrisSpacing.MD))
            Text(text = "RETRY", style = IrisType.Label.copy(color = IrisColors.AccentPrimary))
        }
    }
}

/**
 * HV-59 replacement: smart transport button shown in the status bar.
 *
 * - No active transports → single ⋯ chip
 * - 1 active transport  → that transport's label + state dot
 * - 2+ active transports → first two labels + dots
 *
 * "Active" means Connected, Available, or Degraded (anything except Unavailable).
 * Tapping always opens the full transport sheet.
 */
@Composable
private fun TransportMediaButton(
    transportStates: List<TransportStatus>,
    meshRunning: Boolean,
    onClick: () -> Unit,
) {
    val active = if (meshRunning) transportStates.filter { it.state != "Unavailable" } else emptyList()

    val shape = iriscore.designsystem.IrisRadius.SM
    Row(
        modifier = Modifier
            .clip(shape)
            .border(
                width = 1.dp,
                color = if (active.isEmpty()) IrisColors.BorderSubtle else IrisColors.AccentPrimary.copy(alpha = 0.4f),
                shape = shape,
            )
            .clickable(onClick = onClick)
            .padding(horizontal = IrisSpacing.SM, vertical = IrisSpacing.XXS),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(IrisSpacing.XS),
    ) {
        if (active.isEmpty()) {
            Text(
                text = "⋯",
                style = IrisType.Label.copy(color = IrisColors.TextSecondary),
            )
        } else {
            active.take(2).forEachIndexed { i, t ->
                if (i > 0) {
                    Text(
                        text = "·",
                        style = IrisType.Meta.copy(color = IrisColors.TextSecondary),
                    )
                }
                Text(
                    text = when (t.state) {
                        "Connected" -> "${t.label} ●"
                        "Available" -> "${t.label} ◐"
                        "Degraded"  -> "${t.label} △"
                        else        -> "${t.label} ○"
                    },
                    style = IrisType.Meta.copy(
                        color = when (t.state) {
                            "Connected", "Available" -> IrisColors.AccentPrimary
                            else -> IrisColors.AccentWarning
                        },
                    ),
                )
            }
            if (active.size > 2) {
                Text(
                    text = "+${active.size - 2}",
                    style = IrisType.Meta.copy(color = IrisColors.TextSecondary),
                )
            }
        }
    }
}

/**
 * Bottom sheet listing all transport media.
 *
 * Each row shows the medium's name, current state, and — when Unavailable —
 * an ENABLE action that fires the appropriate OS intent:
 *   BLE  → system Bluetooth-enable dialog
 *   WD / NAN → Wi-Fi settings panel
 *   NET  → Wireless settings (relay is software — can't be toggled like a radio)
 *   other → general wireless settings
 */
@Composable
private fun TransportSheet(
    transportStates: List<TransportStatus>,
    meshRunning: Boolean,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = IrisColors.SurfacePrimary,
    ) {
        Text(
            text = "TRANSPORT MEDIA",
            style = IrisType.Label.copy(color = IrisColors.TextSecondary),
            modifier = Modifier.padding(start = IrisSpacing.MD, end = IrisSpacing.MD, bottom = IrisSpacing.SM),
        )

        if (!meshRunning) {
            Text(
                text = "Mesh not running — tap ENABLE to turn on a radio.",
                style = IrisType.Secondary.copy(color = IrisColors.TextSecondary),
                modifier = Modifier.padding(start = IrisSpacing.MD, end = IrisSpacing.MD, bottom = IrisSpacing.SM),
            )
        }

        // When the mesh is running use live state from the engine; otherwise
        // fall back to a static list of all known transports so the user can
        // still enable radios even before the mesh has started.
        val knownLabels = listOf("BLE", "WD", "NAN", "NET", "LAN")
        val items = if (meshRunning && transportStates.isNotEmpty()) {
            transportStates
        } else {
            knownLabels.map { label -> TransportStatus(label = label, connected = false, state = "Unavailable") }
        }

        items.forEach { t ->
            val isUnavailable = t.state == "Unavailable"
            ListItem(
                headlineContent = {
                    Text(
                        text = transportFullName(t.label),
                        style = IrisType.Body.copy(color = IrisColors.TextPrimary),
                    )
                },
                supportingContent = if (!meshRunning) null else ({
                    Text(
                        text = t.state,
                        style = IrisType.Meta.copy(
                            color = when (t.state) {
                                "Connected", "Available" -> IrisColors.AccentSuccess
                                "Degraded"  -> IrisColors.AccentWarning
                                else        -> IrisColors.TextSecondary
                            },
                        ),
                    )
                }),
                trailingContent = {
                    if (isUnavailable) {
                        TextButton(onClick = {
                            enableTransportMedium(context, t.label)
                            onDismiss()
                        }) {
                            Text(
                                text = "ENABLE",
                                style = IrisType.Label.copy(color = IrisColors.AccentPrimary),
                            )
                        }
                    } else {
                        Text(
                            text = when (t.state) {
                                "Connected" -> "●"
                                "Available" -> "◐"
                                "Degraded"  -> "△"
                                else        -> "○"
                            },
                            style = IrisType.Body.copy(
                                color = when (t.state) {
                                    "Connected", "Available" -> IrisColors.AccentSuccess
                                    else -> IrisColors.AccentWarning
                                },
                            ),
                        )
                    }
                },
                modifier = if (isUnavailable) {
                    Modifier.clickable { enableTransportMedium(context, t.label); onDismiss() }
                } else {
                    Modifier
                },
            )
        }

        Spacer(Modifier.height(IrisSpacing.LG))
    }
}

/** Map short label → human-readable name shown in the transport sheet. */
private fun transportFullName(label: String): String = when (label.uppercase()) {
    "BLE"  -> "Bluetooth Low Energy"
    "WD"   -> "Wi-Fi Direct"
    "NAN"  -> "Wi-Fi Aware (NAN)"
    "NET"  -> "Internet Relay"
    "LAN"  -> "Local Network (TCP)"
    else   -> label
}

/**
 * Fire the OS intent to enable the given transport medium.
 * BLE  → ACTION_REQUEST_ENABLE system dialog
 * WD / NAN / LAN → Wi-Fi settings (the radio gate for both)
 * NET / other     → general wireless settings
 */
private fun enableTransportMedium(context: android.content.Context, label: String) {
    when (label.uppercase()) {
        "BLE" -> {
            val btAdapter = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                context.getSystemService(BluetoothManager::class.java)?.adapter
            } else {
                @Suppress("DEPRECATION")
                android.bluetooth.BluetoothAdapter.getDefaultAdapter()
            }
            if (btAdapter != null && !btAdapter.isEnabled) {
                @Suppress("DEPRECATION")
                context.startActivity(
                    Intent(android.bluetooth.BluetoothAdapter.ACTION_REQUEST_ENABLE).apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    },
                )
            } else {
                context.startActivity(
                    Intent(Settings.ACTION_BLUETOOTH_SETTINGS).apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    },
                )
            }
        }
        "WD", "NAN", "LAN" -> {
            val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                Intent(Settings.Panel.ACTION_WIFI)
            } else {
                @Suppress("DEPRECATION")
                Intent(Settings.ACTION_WIFI_SETTINGS)
            }
            context.startActivity(intent.apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) })
        }
        else -> context.startActivity(
            Intent(Settings.ACTION_WIRELESS_SETTINGS).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) },
        )
    }
}
