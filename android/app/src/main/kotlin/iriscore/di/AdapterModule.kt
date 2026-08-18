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
import iriscore.adapter.AndroidBleTransportAdapter
import iriscore.adapter.AndroidWifiAwareTransportAdapter
import iriscore.adapter.AndroidWifiDirectTransportAdapter
import javax.inject.Singleton

/**
 * Provides the three foreign-trait adapter implementations (AC-4/AC-5):
 * each adapter carries its own Android platform state and lifecycle
 * primitives ([iriscore.adapter.AdapterLifecycle]).
 */
@Module
@InstallIn(SingletonComponent::class)
object AdapterModule {

    @Provides
    @Singleton
    fun provideBleAdapter(@ApplicationContext context: Context): FfiBleAdapter =
        AndroidBleTransportAdapter(context)

    @Provides
    @Singleton
    fun provideWifiAwareAdapter(@ApplicationContext context: Context): FfiWifiAwareAdapter =
        AndroidWifiAwareTransportAdapter(context)

    @Provides
    @Singleton
    fun provideWifiDirectAdapter(@ApplicationContext context: Context): FfiWifiDirectAdapter =
        AndroidWifiDirectTransportAdapter(context)
}