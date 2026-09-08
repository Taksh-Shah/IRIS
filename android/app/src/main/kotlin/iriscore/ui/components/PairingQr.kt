package iriscore.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter

@Composable
fun PairingQr(code: String, modifier: Modifier = Modifier) {
    val bitmap = remember(code) { renderPairingQr(code) }
    Image(
        bitmap = bitmap.asImageBitmap(),
        contentDescription = "IRIS pairing QR code",
        modifier = modifier,
    )
}

internal fun renderPairingQr(code: String, size: Int = 768): Bitmap {
    val matrix = QRCodeWriter().encode(
        code,
        BarcodeFormat.QR_CODE,
        size,
        size,
        mapOf(EncodeHintType.MARGIN to 2),
    )
    val pixels = IntArray(size * size)
    for (y in 0 until size) {
        for (x in 0 until size) {
            pixels[y * size + x] = if (matrix[x, y]) 0xff000000.toInt() else 0xffffffff.toInt()
        }
    }
    return Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
}
