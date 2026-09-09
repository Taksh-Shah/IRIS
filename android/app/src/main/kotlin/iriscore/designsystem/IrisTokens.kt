package iriscore.designsystem

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.dp

/**
 * Spacing scale. One 4dp base unit, so vertical rhythm stays consistent without
 * anyone reaching for an arbitrary number mid-layout.
 */
@Immutable
object IrisSpacing {
    val None = 0.dp
    val Hairline = 1.dp
    val XXS = 2.dp
    val XS = 4.dp
    val SM = 8.dp
    val MD = 12.dp
    val LG = 16.dp
    val XL = 24.dp
    val XXL = 32.dp
    val XXXL = 48.dp

    /** Horizontal page gutter. Generous — density comes from type, not cramming. */
    val Gutter = 20.dp
}

@Immutable
object IrisRadius {
    val SM = RoundedCornerShape(8.dp)
    val MD = RoundedCornerShape(12.dp)
    val LG = RoundedCornerShape(18.dp)
    val XL = RoundedCornerShape(26.dp)

    /** Fully rounded — pills, the input capsule, status chips. */
    val Full = RoundedCornerShape(percent = 50)

    // Chat bubble shapes: one corner is pinched to 4dp to anchor the bubble
    // on its side (the "tail" corner), the others are fully rounded (18dp).

    /** Outbound bubble — right side; bottom-right corner pinched. */
    val OutboundBubble = RoundedCornerShape(
        topStart = 18.dp,
        topEnd = 18.dp,
        bottomStart = 18.dp,
        bottomEnd = 4.dp,
    )

    /** Inbound bubble — left side; bottom-left corner pinched. */
    val InboundBubble = RoundedCornerShape(
        topStart = 18.dp,
        topEnd = 18.dp,
        bottomStart = 4.dp,
        bottomEnd = 18.dp,
    )
}

/**
 * Fixed component metrics. Shared so the input, palette and navigation align to
 * the same optical rhythm on both phone and desktop-width layouts.
 */
@Immutable
object IrisSizing {
    val InputHeight = 52.dp
    val CommandRowHeight = 44.dp
    val NavigationHeight = 60.dp
    val StatusBarHeight = 28.dp

    /** Command palette never grows past this; beyond it, the list scrolls. */
    val PaletteMaxHeight = 320.dp

    /** Width past which the layout earns a second pane. */
    val TabletBreakpoint = 640.dp
    val DesktopBreakpoint = 1000.dp

    /**
     * Maximum width of the reading column.
     *
     * Message text set edge-to-edge on a tablet or an unfolded foldable runs to
     * well over 120 characters a line, which is genuinely hard to read and is
     * the clearest giveaway of a phone layout that was simply stretched. The
     * transcript is centred and capped instead; chrome still spans the window.
     */
    val ReadableContentWidth = 680.dp

    /** Gutter used once the window is wider than [TabletBreakpoint]. */
    val WideGutter = 32.dp
}

/**
 * Glass tuning. Kept low on purpose — the effect should read as material, not
 * as an effect. If a user notices the shader, it is too strong.
 */
@Immutable
object IrisGlass {
    val BlurRadius = 24.dp
    val BlurRadiusStrong = 36.dp

    /** Tint alpha layered over the blur so text stays legible on any backdrop. */
    const val TintAlpha = 0.72f
    const val TintAlphaStrong = 0.86f

    val BorderWidth = 1.dp
}

/**
 * Motion. Short, physical, and never decorative: opacity, scale, blur and
 * translation only. Nothing bounces for its own sake.
 */
@Immutable
object IrisMotion {
    const val Fast = 120
    const val Normal = 200
    const val Slow = 320

    /** Standard ease — quick departure, settled arrival. */
    val Standard: Easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

    /** For elements leaving; slightly faster out than in. */
    val Exit: Easing = CubicBezierEasing(0.4f, 0f, 1f, 1f)

    /** Press/release of a glass surface — damped, no overshoot. */
    fun <T> pressSpring() = spring<T>(
        dampingRatio = Spring.DampingRatioNoBouncy,
        stiffness = Spring.StiffnessMediumLow,
    )

    /** Surface entering — a trace of overshoot reads as physical settling. */
    fun <T> settleSpring() = spring<T>(
        dampingRatio = 0.82f,
        stiffness = Spring.StiffnessMedium,
    )

    /** Scale a glass panel starts from when opening. */
    const val PanelEnterScale = 0.97f

    /** Scale a glass control drops to while held. */
    const val PressScale = 0.975f
}
