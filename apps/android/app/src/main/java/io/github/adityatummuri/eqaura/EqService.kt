package io.github.adityatummuri.eqaura

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.media.AudioManager
import android.media.AudioPlaybackConfiguration
import android.media.audiofx.AudioEffect
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/** Keeps the process, and with it the effects, alive while the equalizer is on. */
class EqService : Service() {
    private val model get() = (application as EqApp).model

    private val sessions = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val id = intent.getIntExtra(AudioEffect.EXTRA_AUDIO_SESSION, AudioEffect.ERROR_BAD_VALUE)
            if (id <= 0) return
            when (intent.action) {
                AudioEffect.ACTION_OPEN_AUDIO_EFFECT_CONTROL_SESSION -> model.sessionOpened(id)
                AudioEffect.ACTION_CLOSE_AUDIO_EFFECT_CONTROL_SESSION -> model.sessionClosed(id)
            }
        }
    }

    /** A good moment to notice and repair effects the system dropped. */
    private val playback = object : AudioManager.AudioPlaybackCallback() {
        override fun onPlaybackConfigChanged(configs: MutableList<AudioPlaybackConfiguration>?) =
            model.checkEffects()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        val filter = IntentFilter().apply {
            addAction(AudioEffect.ACTION_OPEN_AUDIO_EFFECT_CONTROL_SESSION)
            addAction(AudioEffect.ACTION_CLOSE_AUDIO_EFFECT_CONTROL_SESSION)
        }
        ContextCompat.registerReceiver(this, sessions, filter, ContextCompat.RECEIVER_EXPORTED)
        getSystemService(AudioManager::class.java).registerAudioPlaybackCallback(playback, null)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        } else {
            0
        }
        // Always first: Android stops apps that start a service and then do not show it.
        try {
            ServiceCompat.startForeground(this, NOTIFICATION, notification(), type)
        } catch (e: IllegalStateException) {
            // Refused when Android itself restarts the service in the background.
            Log.w(TAG, "foreground", e)
            stopSelf()
            return START_NOT_STICKY
        }
        if (intent?.action == ACTION_OFF) model.setEnabled(false)
        if (!model.state.value.settings.enabled) {
            stopSelf()
            return START_NOT_STICKY
        }
        model.checkEffects()
        return START_STICKY
    }

    override fun onDestroy() {
        unregisterReceiver(sessions)
        getSystemService(AudioManager::class.java).unregisterAudioPlaybackCallback(playback)
        super.onDestroy()
    }

    private fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.channel_name), NotificationManager.IMPORTANCE_LOW).apply {
                description = getString(R.string.channel_text)
                setShowBadge(false)
            },
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val off = PendingIntent.getService(
            this, 1, Intent(this, EqService::class.java).setAction(ACTION_OFF), PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notification_title))
            .setContentText(model.state.value.output.name)
            .setContentIntent(open)
            .setOngoing(true)
            .setShowWhen(false)
            .addAction(Notification.Action.Builder(null, getString(R.string.turn_off), off).build())
            .build()
    }

    companion object {
        private const val CHANNEL = "equalizer"
        private const val NOTIFICATION = 1
        private const val ACTION_OFF = "io.github.adityatummuri.eqaura.OFF"

        /** Also refreshes the notification when the service is running. */
        fun start(context: Context): Boolean = try {
            ContextCompat.startForegroundService(context, Intent(context, EqService::class.java))
            true
        } catch (_: IllegalStateException) {
            // Android 12 and newer refuse this from the background.
            false
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, EqService::class.java))
        }
    }
}

/** Brings the equalizer back after a restart or an update. */
class StartReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val settings = (context.applicationContext as EqApp).model.state.value.settings
        val wanted = when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED -> settings.startAtBoot
            Intent.ACTION_MY_PACKAGE_REPLACED -> true
            else -> false
        }
        if (wanted && settings.enabled) EqService.start(context)
    }
}
