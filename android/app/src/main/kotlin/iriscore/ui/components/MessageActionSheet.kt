package iriscore.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.window.Dialog
import iriscore.R
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.ui.state.DeliveryStatus
import iriscore.ui.state.InboxUiMessage
import iriscore.ui.state.plainLanguage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * WP12: the long-press action sheet for one message row — copy, reply,
 * retry, message info, delete. Which actions actually show depends on the
 * message: reply only makes sense for an inbound row (an outbound row's
 * `senderId` is the recipient, not a peer to reply to — same rule
 * [IrisMessage]'s tap-to-reply already follows); retry only makes sense for
 * an outbound row that is [DeliveryStatus.FAILED] or [DeliveryStatus.EXPIRED]
 * (retrying a DELIVERED or still-QUEUED message would be meaningless or
 * redundant with the outbox's own retry cadence).
 *
 * Deliberately a plain action list, not a destructive-confirmation flow for
 * delete — WP12 lists deletion alongside copy/reply/retry/info as one of the
 * basic interactions to land before anything more elaborate; a confirmation
 * step is worth adding later if accidental deletes turn out to be a problem
 * in practice, not assumed up front.
 */
@Composable
fun MessageActionSheet(
    message: InboxUiMessage,
    onDismiss: () -> Unit,
    onReply: (() -> Unit)?,
    onRetry: (() -> Unit)?,
    onCopied: () -> Unit,
    onShowInfo: () -> Unit,
    onDelete: () -> Unit,
) {
    val context = LocalContext.current

    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .fillMaxWidth()
                .background(IrisColors.SurfacePrimary, IrisRadius.MD)
                .padding(vertical = IrisSpacing.SM),
        ) {
            ActionRow(stringResource(R.string.action_copy)) {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(ClipData.newPlainText("IRIS message", message.payloadUtf8))
                onCopied()
                onDismiss()
            }
            if (onReply != null) {
                ActionRow(stringResource(R.string.action_reply)) {
                    onReply()
                    onDismiss()
                }
            }
            if (onRetry != null) {
                ActionRow(stringResource(R.string.action_retry)) {
                    onRetry()
                    onDismiss()
                }
            }
            ActionRow(stringResource(R.string.action_info)) {
                onShowInfo()
                onDismiss()
            }
            ActionRow(stringResource(R.string.action_delete), tone = StatusTone.Critical) {
                onDelete()
                onDismiss()
            }
            ActionRow(stringResource(R.string.action_dismiss), onClick = onDismiss)
        }
    }
}

@Composable
private fun ActionRow(label: String, tone: StatusTone = StatusTone.Neutral, onClick: () -> Unit) {
    val color = when (tone) {
        StatusTone.Critical -> IrisColors.AccentCritical
        else -> IrisColors.TextPrimary
    }
    Text(
        text = label,
        style = IrisType.Body.copy(color = color),
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .semantics { role = Role.Button }
            .padding(horizontal = IrisSpacing.LG, vertical = IrisSpacing.MD),
    )
}

/**
 * WP12: the "message info" panel — id, from/to, time, and plain-language
 * status, per the [action_info] entry point in [MessageActionSheet].
 */
@Composable
fun MessageInfoDialog(
    message: InboxUiMessage,
    fromLabel: String,
    onDismiss: () -> Unit,
) {
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .fillMaxWidth()
                .background(IrisColors.SurfacePrimary, IrisRadius.MD)
                .padding(IrisSpacing.LG),
        ) {
            Text(text = stringResource(R.string.action_info), style = IrisType.CommandName)
            Spacer(Modifier.height(IrisSpacing.SM))
            Text(text = stringResource(R.string.msg_info_uid, message.uid), style = IrisType.SystemLine)
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(text = stringResource(R.string.msg_info_from, fromLabel), style = IrisType.SystemLine)
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                text = stringResource(R.string.msg_info_time, infoTimeFormat.format(Date(message.receivedAtMs))),
                style = IrisType.SystemLine,
            )
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                text = stringResource(R.string.msg_info_status, message.deliveryStatus.plainLanguage()),
                style = IrisType.SystemLine,
            )
            Spacer(Modifier.height(IrisSpacing.SM))
            ActionRow(stringResource(R.string.action_dismiss), onClick = onDismiss)
        }
    }
}

private val infoTimeFormat = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)
