package iriscore.designsystem.glass

import android.os.Build
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisGlass
import iriscore.designsystem.IrisRadius

/**
 * Rendering tier for glass surfaces.
 *
 * Every visual effect has a fallback (design rule 18). The ladder is
 * `BLUR -> SCRIM`: where the platform can blur we blur, and where it cannot the
 * surface becomes an opaque tinted panel with the same geometry, border and rim
 * light. The scrim tier is not a degraded-looking variant — on true black a
 * tinted panel with a specular top edge still reads as material.
 */
enum class GlassTier {
    /** RenderEffect blur available (API 31+) and transparency permitted. */
    BLUR,

    /** Opaque tinted panel: pre-31, or the user reduced transparency. */
    SCRIM,
}

/** Ambient tier so a screen resolves capability once, not per surface. */
val LocalGlassTier = staticCompositionLocalOf { GlassTier.SCRIM }

/**
 * Resolves the tier for this device and user.
 *
 * Honours the system "remove animations" accessibility preference as a proxy
 * for reduced-transparency: a user who has asked the system to calm down should
 * not be handed a blurred, translucent interface.
 */
@Composable
fun rememberGlassTier(): GlassTier {
    val context = LocalContext.current
    return remember(context) {
        val animationsOff = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            ) == 0f
        }.getOrDefault(false)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && !animationsOff) {
            GlassTier.BLUR
        } else {
            GlassTier.SCRIM
        }
    }
}

/** Provides a resolved tier to a subtree. */
@Composable
fun ProvideGlassTier(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalGlassTier provides rememberGlassTier(), content = content)
}

/**
 * How prominent a glass surface is.
 *
 * Glass is a material hierarchy, not a decoration applied everywhere: content
 * stays solid and dark, interactive chrome is [Regular], and the one primary
 * floating surface in view is [Prominent]. Glazing every message or list row
 * destroys both the hierarchy and the frame budget.
 */
@Immutable
enum class GlassProminence { Regular, Prominent }

/**
 * The single glass primitive. Every glass surface in IRIS goes through here, so
 * the renderer can change without touching call sites.
 *
 * A note on what this does and does not do: Compose's `Modifier.blur` blurs a
 * composable's own content, not what is painted behind it, so this surface does
 * not sample a live backdrop the way a compositor-level effect would. Real
 * backdrop blur is applied by blurring the *content layer* beneath an overlay —
 * see [glassBackdrop] — which is both cheaper and, for a full-screen overlay
 * like the command palette, visually equivalent. The surface itself contributes
 * the tint, border and rim light that make the panel read as a physical edge.
 */
@Composable
fun IrisGlassSurface(
    modifier: Modifier = Modifier,
    shape: Shape = IrisRadius.LG,
    prominence: GlassProminence = GlassProminence.Regular,
    content: @Composable BoxScope.() -> Unit,
) {
    val tier = LocalGlassTier.current
    val strong = prominence == GlassProminence.Prominent

    // Tint is composited over the environment so the panel stays legible
    // regardless of what sits behind it.
    val alpha = when {
        tier == GlassTier.SCRIM -> if (strong) 0.97f else 0.94f
        strong -> IrisGlass.TintAlphaStrong
        else -> IrisGlass.TintAlpha
    }
    val base = if (strong) IrisColors.SurfaceRaised else IrisColors.SurfacePrimary
    val fill = base.copy(alpha = alpha).compositeOver(IrisColors.BackgroundPrimary)

    Box(
        modifier = modifier
            .clip(shape)
            .background(fill)
            // Rim light: a bright top edge fading to nothing gives the panel a
            // lit leading edge and a dark trailing one, which is most of what
            // makes glass read as physical rather than as a flat overlay.
            .background(
                Brush.linearGradient(
                    colors = listOf(
                        IrisColors.GlassRimLight.copy(alpha = if (strong) 0.20f else 0.13f),
                        IrisColors.GlassHighlight.copy(alpha = 0.04f),
                        IrisColors.GlassRimShadow.copy(alpha = 0f),
                    ),
                    start = Offset.Zero,
                    end = Offset(0f, Float.POSITIVE_INFINITY),
                ),
            )
            .border(
                width = IrisGlass.BorderWidth,
                color = if (strong) IrisColors.BorderActive else IrisColors.BorderSubtle,
                shape = shape,
            ),
        content = content,
    )
}

/**
 * Blurs a content layer sitting *behind* an overlay.
 *
 * This is the backdrop half of the glass effect: the palette or sheet stays
 * sharp while the conversation behind it recedes. Applied to the content layer
 * rather than the overlay because that is the direction Compose's blur actually
 * works, and because blurring one large layer once is far cheaper than giving
 * every floating surface its own backdrop capture.
 *
 * A no-op below API 31 and on the [GlassTier.SCRIM] tier, where the overlay's
 * opaque panel is doing the separation instead.
 */
@Composable
fun Modifier.glassBackdrop(
    active: Boolean,
    radius: Dp = IrisGlass.BlurRadius,
): Modifier {
    val tier = LocalGlassTier.current
    return if (active && tier == GlassTier.BLUR) this.blur(radius) else this
}
