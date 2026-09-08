package iriscore.ui.state

sealed interface PairingUiState {
    data object Idle : PairingUiState
    data object Working : PairingUiState
    data class Preview(val peerIdHex: String, val sas: String) : PairingUiState
    data class Saved(val peerIdHex: String, val alias: String) : PairingUiState
    data class Error(val message: String) : PairingUiState
}
