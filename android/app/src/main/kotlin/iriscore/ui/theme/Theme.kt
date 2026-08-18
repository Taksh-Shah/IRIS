package iriscore.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val IrisTeal = Color(0xFF2DD4BF)
private val IrisInk = Color(0xFF0F172A)

private val LightColors = lightColorScheme(
    primary = IrisTeal,
    onPrimary = IrisInk,
    background = Color(0xFFF8FAFC),
    surface = Color(0xFFFFFFFF),
)

private val DarkColors = darkColorScheme(
    primary = IrisTeal,
    onPrimary = IrisInk,
    background = Color(0xFF0F172A),
    surface = Color(0xFF1E293B),
)

@Composable
fun IrisTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = if (darkTheme) DarkColors else LightColors,
        content = content,
    )
}