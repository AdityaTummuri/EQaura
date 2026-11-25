import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

// One version number is shared with the desktop apps.
val appVersion: String = Regex("""(?m)^version = "(.+)"""")
    .find(file("../../../Cargo.toml").readText())!!.groupValues[1]
val keystore: String? = System.getenv("EQAURA_KEYSTORE")

android {
    namespace = "io.github.adityatummuri.eqaura"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.adityatummuri.eqaura"
        minSdk = 28
        targetSdk = 36
        versionName = appVersion
        versionCode = appVersion.split('.', '-').take(3).map(String::toInt)
            .let { (major, minor, patch) -> major * 1_000_000 + minor * 1_000 + patch }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("EQAURA_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("EQAURA_KEY_ALIAS")
                keyPassword = System.getenv("EQAURA_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Without a key of our own the build can still be installed and tried.
            signingConfig = signingConfigs.getByName(if (keystore != null) "release" else "debug")
        }
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    lint {
        warningsAsErrors = true
        // New library versions must not break an unchanged build.
        disable += setOf(
            "GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "OldTargetApi",
        )
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
        jniLibs.useLegacyPackaging = false
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
        allWarningsAsErrors = true
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.10.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")

    androidTestImplementation(platform("androidx.compose:compose-bom:2025.10.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:rules:1.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
