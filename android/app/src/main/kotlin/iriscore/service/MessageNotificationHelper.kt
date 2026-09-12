package iriscore.service

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import iriscore.R
import iriscore.ui.MainActivity

/**
 * WP9 — message notification channel + post helper.
 *
 * [createChannel] must be called before any notification is posted (safe to
 * call repeatedly — Android deduplicates channel creation). [postMessageNotification]
 * checks the POST_NOTIFICATIONS runtime permission (API 33+) before posting so
 * the call-site does not need to guard it.
 */
object MessageNotificationHelper {

    const val CHANNEL_ID = "iris_messages"

    /**
     * Base for per-sender notification IDs. The actual ID is derived from the
     * first 8 hex characters of the sender's peer-id so each peer gets a stable
     * slot that updates in place rather than stacking indefinitely.
     */
    private const val BASE_NOTIF_ID = 1000

    fun createChannel(context: Context) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.msg_channel_name),
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = context.getString(R.string.msg_channel_description)
            }
            context.getSystemService(NotificationManager::class.java)
                .createNotificationChannel(channel)
        }
    }

    /**
     * Post a notification for a newly arrived inbound message.
     *
     * @param senderHex   The sender's full peer-id hex string.
     * @param senderName  The user's saved contact alias for this sender
     *                    ([iriscore.data.ContactStore.resolve]), or `null` if
     *                    none is saved — falls back to the hex prefix, same as
     *                    the in-app message list ([iriscore.ui.components.IrisMessage]).
     * @param preview     A short (≤80 char) preview of the message payload.
     */
    fun postMessageNotification(context: Context, senderHex: String, senderName: String?, preview: String) {
        // POST_NOTIFICATIONS is a runtime permission on API 33+; bail silently if
        // not granted (the user declined it in MainActivity).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED
            ) return
        }

        val openIntent = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            context,
            0,
            openIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        // Bug fix: the message preview used to live only in BigTextStyle's
        // bigText, which Android shows only once a notification is expanded
        // -- whether that happens by default is a per-OEM/per-version
        // rendering choice (heads-up banners, MIUI's own style, and larger
        // screens often auto-expand; plain collapsed notifications never
        // do). That made the same code show the message body on some
        // devices and just "From <sender>" on others. contentText is always
        // visible in the collapsed form, so put the actual preview there on
        // every device; BigTextStyle still carries the same preview so a
        // long message just gets more room once expanded, not different
        // content depending on rendering.
        val title = context.getString(
            R.string.msg_notification_title_template,
            senderName ?: senderHex.take(8),
        )

        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(preview)
            .setStyle(NotificationCompat.BigTextStyle().bigText(preview).setBigContentTitle(title))
            .setSmallIcon(R.drawable.ic_stat_iris)
            .setContentIntent(pendingIntent)
            .setAutoCancel(true)
            .build()

        val notifId = BASE_NOTIF_ID + (senderHex.take(8).toLongOrNull(16)?.and(0x7FFFFFFF)?.toInt() ?: 0)

        NotificationManagerCompat.from(context).notify(notifId, notification)
    }
}
