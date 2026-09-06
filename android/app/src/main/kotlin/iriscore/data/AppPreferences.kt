package iriscore.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * HV-62: lightweight app-level preference flags backed by SharedPreferences.
 *
 * Used for one-time UX states that do not belong in the mesh data layer —
 * currently the first-run welcome flag. Booleans are stored atomically via
 * `apply()` (async, fire-and-forget — appropriate for non-critical flags).
 */
@Singleton
class AppPreferences @Inject constructor(
    @ApplicationContext context: Context,
) {
    private val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    /** True after the first-launch WELCOME event has been emitted once. */
    var welcomeShown: Boolean
        get() = prefs.getBoolean(KEY_WELCOME_SHOWN, false)
        set(v) { prefs.edit().putBoolean(KEY_WELCOME_SHOWN, v).apply() }

    private companion object {
        const val PREFS_NAME = "iris_app_prefs"
        const val KEY_WELCOME_SHOWN = "welcome_shown"
    }
}
