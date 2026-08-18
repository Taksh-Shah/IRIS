# IRIS Kotlin Android Layer

## Architecture Pattern

IRIS Android uses MVVM + Clean Architecture. The architecture separates UI concerns from business logic and keeps the Rust FFI confined to the data source layer.

```
UI (Compose) → ViewModel → UseCase → Repository → DataSource
                                                  ├── IrisCoreDataSource (Rust FFI)
                                                  └── LocalDataSource (Room)
```

- **UI**: Jetpack Compose screens. No business logic. Observes ViewModel state.
- **ViewModel**: Holds UI state (StateFlow/SharedFlow). Calls use cases. Survives configuration changes.
- **UseCase**: Single-responsibility business operation. Orchestrates repositories.
- **Repository**: Abstracts data source selection. Handles caching logic.
- **DataSource**: Actual data retrieval — either Rust core (via FFI) or Room (local metadata).

## UniFFI Binding Layer

> **Conformance note (ANDROID-001, iter 116):** the older sketches in this doc
> (`IrisCore`, `IrisCore.initialize()`, `AndroidBleTransportAdapter.registerTransport(...)`)
> are pre-AC-2/AC-4 reference code. The implemented binding surface is what this
> section and `ANDROID.md §UniFFI Integration` describe: generated `IrisEngine`
> + foreign-trait adapters, with no hand-written JNI beyond `System.loadLibrary`.

**Bound to `crates/iris-android` ("iriscode" library) via UniFFI 0.31.2** —
foreign-trait callback interfaces, not the soft-deprecated UDL callback
interfaces:

1. **Generated Kotlin is committed** (kept independent of the Rust/NDK
   toolchain on CI): `kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt` —
   `IrisEngine` (`#[derive(uniffi::Object)]`, owns a tokio `Runtime`), the three
   foreign-trait adapter interfaces (`FfiBleAdapter` sync 10-op,
   `FfiWifiAwareAdapter` + `FfiWifiDirectAdapter` async → generated Kotlin
   `suspend fun`), `IrisFfiException` + data classes.
2. **Package facade (AC-11 conformance seam):**
   `kotlin/src/main/kotlin/iriscode/api.kt` re-exports `uniffi.iriscode` under
   package `iriscode` so every adapter/shell file imports the single stable
   namespace `import iriscode.*` — no direct `uniffi.*` imports leak into app
   code.
3. **Foreign-trait injection:** Kotlin adapter classes under
   `iriscore/adapter/` implement the three interfaces; `IrisEngine(ble, aware,
   direct, nodeId)` is constructed once in `IrisCoreModule`. Async adapter calls
   cross the FFI by UniFFI's foreign-future / oneshot callback mechanism and are
   polled through an **explicit tokio runtime handle** (`runtime.block_on`) —
   the proven workaround for UniFFI issue #2576
   (`#[uniffi::export(async_runtime="tokio")]` is ineffective on exported-trait
   impls).
4. **Engine surface used by the shell:** `start_all` / `stop_all` / `send_text`
   / `subscribe_inbox` (`FfiInboxListener` + `FfiIncomingMessage`),
   `drainBleScanResults` / `drainIncomingNdp` (engine polls adapter drains under
   its runtime); `parse_peer_id_hex` / `build_text_envelope` helpers. UI never
   talks to the engine directly — `MeshRepository` wraps
   `FfiInboxListener` → `StateFlow` and owns the bounded `RelayOutbox`.

## Dependency Injection: Hilt

All dependencies are provided via Hilt modules. No manual dependency construction in production code.

```kotlin
@HiltAndroidApp
class IrisApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // Load Rust native library
        System.loadLibrary("iriscode")
        IrisCore.initialize()
    }
}

@Module
@InstallIn(SingletonComponent::class)
object IrisCoreModule {
    @Provides
    @Singleton
    fun provideIrisCore(@ApplicationContext context: Context): IrisCore {
        val config = IrisCoreConfig(
            storagePath = context.filesDir.absolutePath + "/iris.db",
            routingAlgorithm = "prophet",
            maxStorageBytes = 500_000_000u,
            emergencyEnabled = true,
        )
        return IrisCore(config)
    }

    @Provides
    @Singleton
    fun provideIrisCoreDataSource(irisCore: IrisCore): IrisCoreDataSource =
        IrisCoreDataSource(irisCore)
}

@Module
@InstallIn(SingletonComponent::class)
object ServiceModule {
    @Provides
    @Singleton
    fun provideIrisBleServiceController(
        @ApplicationContext context: Context
    ): IrisBleServiceController = IrisBleServiceController(context)
}
```

## ViewModel Example

```kotlin
@HiltViewModel
class MessagingViewModel @Inject constructor(
    private val getMessagesUseCase: GetMessagesUseCase,
    private val sendMessageUseCase: SendMessageUseCase,
    private val getNetworkStatusUseCase: GetNetworkStatusUseCase,
) : ViewModel() {

    private val _uiState = MutableStateFlow(MessagingUiState.Loading)
    val uiState: StateFlow<MessagingUiState> = _uiState.asStateFlow()

    init {
        loadMessages()
        observeNetworkStatus()
    }

    private fun loadMessages() {
        viewModelScope.launch {
            getMessagesUseCase.execute()
                .catch { e ->
                    _uiState.update { MessagingUiState.Error(e.message ?: "Unknown error") }
                }
                .collect { messages ->
                    _uiState.update { MessagingUiState.Success(messages) }
                }
        }
    }

    fun sendMessage(recipientId: String, text: String) {
        viewModelScope.launch {
            try {
                sendMessageUseCase.execute(recipientId, text.encodeToByteArray(), priority = 4u)
            } catch (e: IrisCoreException) {
                _uiState.update { state ->
                    (state as? MessagingUiState.Success)?.copy(sendError = e.message) ?: state
                }
            }
        }
    }

    private fun observeNetworkStatus() {
        viewModelScope.launch {
            getNetworkStatusUseCase.execute()
                .collect { status ->
                    _uiState.update { state ->
                        (state as? MessagingUiState.Success)?.copy(networkStatus = status) ?: state
                    }
                }
        }
    }
}

sealed class MessagingUiState {
    object Loading : MessagingUiState()
    data class Success(
        val messages: List<MessageSummary>,
        val networkStatus: NetworkStatus = NetworkStatus.Unknown,
        val sendError: String? = null,
    ) : MessagingUiState()
    data class Error(val message: String) : MessagingUiState()
}
```

## Compose UI

```kotlin
@Composable
fun MessagingScreen(
    viewModel: MessagingViewModel = hiltViewModel(),
    onNavigateToSos: () -> Unit,
) {
    val uiState by viewModel.uiState.collectAsStateWithLifecycle()

    Scaffold(
        topBar = { IrisTopBar(uiState) },
        floatingActionButton = {
            SosButton(onClick = onNavigateToSos)
        }
    ) { paddingValues ->
        when (val state = uiState) {
            MessagingUiState.Loading -> LoadingIndicator()
            is MessagingUiState.Error -> ErrorState(state.message)
            is MessagingUiState.Success -> MessageList(
                messages = state.messages,
                modifier = Modifier.padding(paddingValues)
            )
        }
    }
}

@Composable
fun SosButton(onClick: () -> Unit) {
    ExtendedFloatingActionButton(
        onClick = onClick,
        containerColor = MaterialTheme.colorScheme.error,
        contentColor = MaterialTheme.colorScheme.onError,
        icon = { Icon(Icons.Default.Warning, contentDescription = null) },
        text = { Text("SOS") },
    )
}
```

## BLE Foreground Service

```kotlin
@AndroidEntryPoint
class IrisBleService : Service() {

    @Inject lateinit var irisCore: IrisCore
    @Inject lateinit var bleTransportAdapter: AndroidBleTransportAdapter

    private val binder = LocalBinder()
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    inner class LocalBinder : Binder() {
        fun getService(): IrisBleService = this@IrisBleService
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, buildNotification())
        startBleOperations()
        return START_STICKY
    }

    private fun startBleOperations() {
        serviceScope.launch {
            bleTransportAdapter.start()
            // Register transport with Rust core
            irisCore.registerTransport(bleTransportAdapter)
        }
    }

    private fun buildNotification(): Notification {
        val channel = NotificationChannel(
            CHANNEL_ID, "IRIS Relay",
            NotificationManager.IMPORTANCE_LOW
        ).apply { description = "IRIS is relaying messages in the background" }
        notificationManager.createNotificationChannel(channel)

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("IRIS Active")
            .setContentText("Mesh relay running")
            .setSmallIcon(R.drawable.ic_iris_notification)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceScope.cancel()
        bleTransportAdapter.stop()
        scheduleWorkManagerFallback()
    }

    private fun scheduleWorkManagerFallback() {
        val request = PeriodicWorkRequestBuilder<IrisBackgroundWorker>(15, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder()
                .setRequiresBatteryNotLow(false)
                .build())
            .build()
        WorkManager.getInstance(applicationContext)
            .enqueueUniquePeriodicWork("iris-bg", ExistingPeriodicWorkPolicy.REPLACE, request)
    }

    companion object {
        const val NOTIFICATION_ID = 1001
        const val CHANNEL_ID = "iris_relay"
    }
}
```

## Android BLE Transport Adapter

The `AndroidBleTransportAdapter` bridges Android BLE APIs to the Rust `TransportAdapter` interface:

```kotlin
class AndroidBleTransportAdapter @Inject constructor(
    @ApplicationContext private val context: Context,
    private val irisCore: IrisCore,
) {
    private val bluetoothManager = context.getSystemService(BluetoothManager::class.java)
    private val bluetoothAdapter = bluetoothManager.adapter
    private val scanner by lazy { bluetoothAdapter.bluetoothLeScanner }
    private val advertiser by lazy { bluetoothAdapter.bluetoothLeAdvertiser }

    private var gattServer: BluetoothGattServer? = null
    private val activeConnections = ConcurrentHashMap<String, BluetoothGatt>()

    suspend fun start() = withContext(Dispatchers.IO) {
        startGattServer()
        startAdvertising()
        startScanning()
    }

    private fun startGattServer() {
        gattServer = bluetoothManager.openGattServer(context, gattServerCallback)
        val service = BluetoothGattService(IRIS_SERVICE_UUID, SERVICE_TYPE_PRIMARY)
        service.addCharacteristic(messageWriteCharacteristic())
        service.addCharacteristic(messageNotifyCharacteristic())
        gattServer?.addService(service)
    }

    // Called by BluetoothGattServerCallback when remote device writes
    private fun onMessageReceived(data: ByteArray, senderAddress: String) {
        // Forward to Rust core via FFI
        irisCore.handleIncomingTransportData(
            transportType = "ble",
            peerId = senderAddress,
            data = data,
        )
    }
}
```

## Room Database (Metadata)

The Room database stores message metadata for UI display. Message payloads and routing data live in the Rust/SQLCipher database.

```kotlin
@Database(
    entities = [MessageMetadataEntity::class],
    version = 1,
    exportSchema = true,
)
abstract class IrisMetadataDatabase : RoomDatabase() {
    abstract fun messageMetadataDao(): MessageMetadataDao
}

@Entity(tableName = "message_metadata")
data class MessageMetadataEntity(
    @PrimaryKey val messageId: String,
    val senderDisplayName: String?,
    val previewText: String,
    val priority: Int,
    val status: String,
    val createdAtMs: Long,
    val isIncoming: Boolean,
)

@Dao
interface MessageMetadataDao {
    @Query("SELECT * FROM message_metadata ORDER BY createdAtMs DESC LIMIT :limit")
    fun getRecent(limit: Int): Flow<List<MessageMetadataEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(entity: MessageMetadataEntity)

    @Query("UPDATE message_metadata SET status = :status WHERE messageId = :id")
    suspend fun updateStatus(id: String, status: String)
}
```

## Emergency Notification

```kotlin
fun createEmergencyNotificationChannel(context: Context) {
    val channel = NotificationChannel(
        "iris_emergency",
        "Emergency Alerts",
        NotificationManager.IMPORTANCE_HIGH
    ).apply {
        description = "Critical emergency broadcasts from IRIS network"
        enableLights(true)
        lightColor = Color.RED
        enableVibration(true)
        vibrationPattern = longArrayOf(0, 500, 200, 500, 200, 500)
        lockscreenVisibility = Notification.VISIBILITY_PUBLIC
    }
    val notificationManager = context.getSystemService(NotificationManager::class.java)
    notificationManager.createNotificationChannel(channel)
}

fun showSosReceivedNotification(context: Context, senderName: String) {
    val notification = NotificationCompat.Builder(context, "iris_emergency")
        .setContentTitle("🚨 SOS Received")
        .setContentText("$senderName has sent an SOS signal")
        .setSmallIcon(R.drawable.ic_sos)
        .setPriority(NotificationCompat.PRIORITY_MAX)
        .setCategory(NotificationCompat.CATEGORY_ALARM)
        .setAutoCancel(true)
        .setFullScreenIntent(sosPendingIntent(context), true)
        .build()
    NotificationManagerCompat.from(context).notify(SOS_NOTIFICATION_ID, notification)
}
```

## Testing

```kotlin
// ViewModel test with fake use case
@Test
fun `send message updates ui state`() = runTest {
    val fakeSendMessage = FakeSendMessageUseCase()
    val viewModel = MessagingViewModel(
        getMessagesUseCase = FakeGetMessagesUseCase(),
        sendMessageUseCase = fakeSendMessage,
        getNetworkStatusUseCase = FakeGetNetworkStatusUseCase(),
    )

    viewModel.sendMessage("recipient123", "Hello mesh")

    assertTrue(fakeSendMessage.wasCalledWith("recipient123"))
}

// Integration test with in-memory Room
@Test
fun `message metadata persisted after receipt`() = runTest {
    val db = Room.inMemoryDatabaseBuilder(
        ApplicationProvider.getApplicationContext(),
        IrisMetadataDatabase::class.java
    ).build()

    val dao = db.messageMetadataDao()
    dao.upsert(MessageMetadataEntity(
        messageId = "test-id",
        senderDisplayName = "Alice",
        previewText = "Hello",
        priority = 4,
        status = "RECEIVED",
        createdAtMs = System.currentTimeMillis(),
        isIncoming = true,
    ))

    val messages = dao.getRecent(10).first()
    assertEquals(1, messages.size)
    assertEquals("Hello", messages[0].previewText)
    db.close()
}
```
