package iriscore.ui.navigation

import androidx.compose.runtime.Composable
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import iriscore.ui.screens.ConsoleScreen
import iriscore.ui.screens.PairingScreen
import iriscore.ui.screens.SettingsScreen

object IrisRoutes {
    const val CHAT = "chat"
    const val SETTINGS = "settings"
    const val PAIRING = "pairing"
}

@Composable
fun IrisNavGraph(navController: NavHostController) {
    NavHost(
        navController = navController,
        startDestination = IrisRoutes.CHAT,
    ) {
        composable(IrisRoutes.CHAT) {
            ConsoleScreen()
        }
        composable(IrisRoutes.SETTINGS) {
            SettingsScreen(
                onNavigateToPairing = { navController.navigate(IrisRoutes.PAIRING) },
                onBack = { navController.popBackStack() },
            )
        }
        composable(IrisRoutes.PAIRING) {
            PairingScreen(onBack = { navController.popBackStack() })
        }
    }
}
