package iriscore.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.ui.state.DeliveryStatus
import iriscore.ui.state.InboxUiMessage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * A message.
 *
 * Editorial rather than bubbled: sender, body, and a quiet timestamp, carried
 * by type and spacing. Chat bubbles would put a container around every line and
 * make IRIS read as a consumer messenger; here the only containers are the ones
 * that earn their place.
 */
@Composable
fun IrisMessage(
    message: InboxUiMessage,
    /**
     * True when the previous message came from the same sender, in which case
     * the name is dropped and the spacing tightens — a run of messages reads as
     * one utterance rather than a repeated header.
     */
    grouped: Boolean,
    modifier: Modifier = Modifier,
    /** HV-56: resolved contact name for the sender; null shows the raw hex prefix. */
    contactName: String? = null,
    /**
     * HV-57: tapping a received message sets it as the reply recipient — the
     * same effect as `/to <peerId>` by hand. Null keeps the row inert (e.g.
     * an outgoing echo of your own sent message, if this composable is ever
     * reused for that).
     */
    onReply: ((senderId: String) -> Unit)? = null,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = if (grouped) IrisSpacing.XS else IrisSpacing.LG)
            .then(
                if (onReply != null) {
                    Modifier
                        .clickable(onClickLabel = "Reply") { onReply(message.senderId) }
                        .semantics {
                            role = Role.Button
                            contentDescription = "Reply to ${message.senderId.take(12)}"
                        }
                } else {
                    Modifier
                },
            ),
    ) {
        if (!grouped) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = contactName ?: message.senderId.take(12),
                    style = IrisType.Sender,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (message.priority == PRIORITY_SOS) {
                    Spacer(Modifier.width(IrisSpacing.SM))
                    IrisStatusChip(label = "P0", tone = StatusTone.Critical)
                }
                // HV-58: delivery status chip for outbound messages.
                when (message.deliveryStatus) {
                    DeliveryStatus.QUEUED -> {
                        Spacer(Modifier.width(IrisSpacing.SM))
                        IrisStatusChip(label = "QUEUED", tone = StatusTone.Warning)
                    }
                    DeliveryStatus.FAILED -> {
                        Spacer(Modifier.width(IrisSpacing.SM))
                        IrisStatusChip(label = "FAILED", tone = StatusTone.Critical)
                    }
                    else -> Unit
                }
            }
            Spacer(Modifier.height(IrisSpacing.XS))
        }

        Text(text = message.payloadUtf8, style = IrisType.Body)

        Spacer(Modifier.height(IrisSpacing.XXS))
        Text(text = formatTime(message.receivedAtMs), style = IrisType.Meta)
    }
}

private const val PRIORITY_SOS: UByte = 0u

private val timeFormat = SimpleDateFormat("HH:mm:ss", Locale.US)

private fun formatTime(epochMs: Long): String = timeFormat.format(Date(epochMs))

/**
 * A system event block.
 *
 * This is where IRIS speaks as a system rather than as a participant: an
 * all-caps mono label, aligned `> key   value` lines, and a terminal status.
 * Kept visually quiet — the intelligence should read as part of the interface,
 * not as a card announcing itself.
 */
@Composable
fun IrisSystemEvent(
    title: String,
    lines: List<Pair<String, String>>,
    status: String? = null,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth().padding(top = IrisSpacing.LG)) {
        Text(text = "IRIS", style = IrisType.Label)
        Spacer(Modifier.height(IrisSpacing.XS))
        Text(text = title, style = IrisType.CommandName)

        if (lines.isNotEmpty()) {
            Spacer(Modifier.height(IrisSpacing.SM))
            // The key column is padded to a fixed width so values align down the
            // block the way they would in a terminal.
            val keyWidth = lines.maxOf { it.first.length }
            lines.forEach { (key, value) ->
                Text(
                    text = "> ${key.padEnd(keyWidth)}   $value",
                    style = IrisType.SystemLine,
                )
            }
        }

        if (status != null) {
            Spacer(Modifier.height(IrisSpacing.SM))
            Text(
                text = "STATUS: $status",
                style = IrisType.Label.copy(color = IrisColors.TextSecondary),
            )
        }
    }
}
