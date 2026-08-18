package iriscore.di

import android.content.Context
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import iriscode.FfiBleAdapter
import iriscode.FfiWifiAwareAdapter
import iriscode.FfiWifiDirectAdapter
import iriscode.IrisEngine
import iriscore.identity.KeystoreEd25519
import javax.inject.Singleton

/**
 * Engages the Rust engine: provisions the identity (AC-8) and builds [IrisEngine]
 * over the three injected adapters (constructor contract = generated Kotlin:
 * `(ble, aware, direct, nodeId: ByteArray)`).
 */
@Module
@InstallIn(SingletonComponent::class)
object IrisCoreModule {

    @Provides
    @Singleton
    fun provideKeystoreEd25519(@ApplicationContext context: Context): KeystoreEd25519 =
        KeystoreEd25519(context)

    /** The 32-byte identity public key doubles as the engine node id. */
    @Provides
    @Singleton
    @NodeId
    fun provideNodeId(keystore: KeystoreEd25519): ByteArray = keystore.publicKeyRaw()

    @Provides
    @Singleton
    fun provideIrisEngine(
        ble: FfiBleAdapter,
        aware: FfiWifiAwareAdapter,
        direct: FfiWifiDirectAdapter,
        @NodeId nodeId: ByteArray,
    ): IrisEngine = IrisEngine(ble, aware, direct, nodeId)
}