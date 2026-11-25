package io.github.adityatummuri.eqaura

import android.app.Application

class EqApp : Application() {
    lateinit var model: AppModel
        private set

    override fun onCreate() {
        super.onCreate()
        model = AppModel(this)
    }
}
