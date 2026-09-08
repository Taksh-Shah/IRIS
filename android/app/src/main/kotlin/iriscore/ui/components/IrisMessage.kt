package iriscore.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.ui.text.style.TextAlign
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
 * Direction-aware message row.
 *
 * Inbound (isOutbound=false): left-aligned, sender name or hex prefix as label.
 * Outbound (isOutbound=true): right-aligned, "You" as label, delivery chip in header.
 *
 * Editorial rather than bubbled — type and spacing carry the distinction, not containers.
 */
@Composable
fun IrisMessage(
    message: InboxUiMessage,
    /**
     * True when the previous message had the same sender and same direction,
     * so the name header is suppressed and spacing tightens.
     */
    grouped: Boolean,
    modifier: Modifier = Modifier,
    contactName: String? = null,
    /** Tapping a received message sets it as the reply recipient. Null = inert row. */
    onReply: ((senderId: String) -> Unit)? = null,
) {
    val outbound = message.isOutbound
    val horizontalAlignment = if (outbound) Alignment.End else Alignment.Start
    val textAlign = if (outbound) TextAlign.End else TextAlign.Start

    Column(
        horizontalAlignment = horizontalAlignment,
        modifier = modifier
            .fillMaxWidth()
            .padding(top = if (grouped) IrisSpacing.XS else IrisSpacing.LG)
            .then(
                if (!outbound && onReply != null) {
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
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = if (outbound) Arrangement.End else Arrangement.Start,
            ) {
                if (!outbound && message.priority == PRIORITY_SOS) {
                    IrisStatusChip(label = "P0", tone = StatusTone.Critical)
                    Spacer(Modifier.width(IrisSpacing.SM))
                }
                if (outbound) {
                    // Delivery chip before the "You" label for right-to-left read order.
                    when (message.deliveryStatus) {
                        DeliveryStatus.QUEUED -> {
                            IrisStatusChip(label = "QUEUED", tone = StatusTone.Warning)
                            Spacer(Modifier.width(IrisSpacing.SM))
                        }
                        DeliveryStatus.FAILED -> {
                            IrisStatusChip(label = "FAILED", tone = StatusTone.Critical)
                            Spacer(Modifier.width(IrisSpacing.SM))
                        }
                        DeliveryStatus.DELIVERED -> {
                            IrisStatusChip(label = "SENT", tone = StatusTone.Active)
                            Spacer(Modifier.width(IrisSpacing.SM))
                        }
                        else -> Unit
                    }
                    if (message.priority == PRIORITY_SOS) {
                        IrisStatusChip(label = "P0", tone = StatusTone.Critical)
                        Spacer(Modifier.width(IrisSpacing.SM))
                    }
                }
                Text(
                    text = if (outbound) "You" else (contactName ?: message.senderId.take(12)),
                    style = IrisType.Sender.let {
                        if (outbound) it.copy(color = IrisColors.AccentPrimary) else it
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.height(IrisSpacing.XS))
        }

        Text(
            text = message.payloadUtf8,
            style = IrisType.Body,
            textAlign = textAlign,
        )

        Spacer(Modifier.height(IrisSpacing.XXS))
        Text(
            text = formatTime(message.receivedAtMs),
            style = IrisType.Meta,
            textAlign = textAlign,
        )
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
