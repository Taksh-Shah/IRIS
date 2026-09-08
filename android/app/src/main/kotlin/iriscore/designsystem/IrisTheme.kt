package iriscore.designsystem

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

/**
 * IRIS Material 3 theme wrapper.
 *
 * Maps the IRIS palette onto the M3 color role slots so that Material
 * components (dialogs, buttons, text fields, chips) used anywhere in the app
 * receive coherent colors without needing per-call overrides. Custom design
 * tokens (IrisColors, IrisType, IrisSpacing) continue to be used directly
 * in custom composables — this wrapper only ensures that any M3 component
 * that appears (pairing dialog, etc.) inherits the correct dark palette
 * rather than the M3 default purple/teal.
 */
@Composable
fun IrisTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            background        = IrisColors.BackgroundPrimary,
            surface           = IrisColors.SurfacePrimary,
            surfaceVariant    = IrisColors.SurfaceSecondary,
            surfaceContainer  = IrisColors.SurfaceRaised,
            primary           = IrisColors.AccentPrimary,
            onPrimary         = IrisColors.BackgroundPrimary,
            secondary         = IrisColors.AccentSuccess,
            onSecondary       = IrisColors.BackgroundPrimary,
            tertiary          = IrisColors.AccentWarning,
            onTertiary        = IrisColors.BackgroundPrimary,
            error             = IrisColors.AccentCritical,
            onError           = IrisColors.BackgroundPrimary,
            onBackground      = IrisColors.TextPrimary,
            onSurface         = IrisColors.TextPrimary,
            onSurfaceVariant  = IrisColors.TextSecondary,
            outline           = IrisColors.BorderSubtle,
            outlineVariant    = IrisColors.BorderActive,
        ),
        content = content,
    )
}
