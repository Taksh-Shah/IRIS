package iriscore.di

import android.content.Context
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import iriscode.FfiBleAdapter
import iriscode.FfiCryptoSigner
import iriscode.FfiWifiAwareAdapter
import iriscode.FfiWifiDirectAdapter
import iriscode.FfiX25519KeyProvider
import iriscode.IrisEngine
import iriscore.data.RelayOutbox
import iriscore.identity.KeystoreEd25519
import iriscore.identity.X25519KeyProviderImpl
import iriscore.identity.X25519StaticAd
import javax.inject.Singleton

/**
 * Engages the Rust engine: provisions the identity (AC-8) and builds [IrisEngine]
 * over the three injected adapters, the Keystore-backed [FfiCryptoSigner]
 * (AN-1: real Ed25519 signing), and the [FfiX25519KeyProvider] (AN-6: real X25519
 * key-agreement, DH performed inside JCA so the static secret never crosses FFI).
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

    /**
     * AN-1: bridges the Android Keystore Ed25519 signing key into the Rust engine.
     * The private key never leaves Kotlin/Keystore TEE — Rust calls `sign(data)`
     * and receives the 64-byte raw signature back over the FFI boundary.
     */
    @Provides
    @Singleton
    fun provideFfiCryptoSigner(keystore: KeystoreEd25519): FfiCryptoSigner =
        object : FfiCryptoSigner {
            override fun sign(data: ByteArray): ByteArray = keystore.sign(data)
        }

    /**
     * Shared relay spool. The UI send path and the WorkManager drain cadence
     * must observe the same queue, so this is a singleton — a per-injection
     * instance would silently strand queued messages in a dead copy.
     */
    @Provides
    @Singleton
    fun provideRelayOutbox(): RelayOutbox = RelayOutbox()

    /**
     * AN-6: bridges [X25519StaticAd] into the Rust engine. DH is computed
     * inside JCA via [X25519KeyProviderImpl]; the raw static secret never
     * crosses the FFI boundary.
     */
    @Provides
    @Singleton
    fun provideFfiX25519KeyProvider(keystore: KeystoreEd25519): FfiX25519KeyProvider =
        X25519KeyProviderImpl(X25519StaticAd(keystore))

    @Provides
    @Singleton
    fun provideIrisEngine(
        ble: FfiBleAdapter,
        aware: FfiWifiAwareAdapter,
        direct: FfiWifiDirectAdapter,
        @NodeId nodeId: ByteArray,
        signer: FfiCryptoSigner,
        x25519Provider: FfiX25519KeyProvider,
    ): IrisEngine = IrisEngine.newWithX25519(ble, aware, direct, nodeId, signer, x25519Provider)
}