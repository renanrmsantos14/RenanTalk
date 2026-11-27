// Versão vem do workflow de release (-PversaoNome=0.0.N -PversaoData="dd/mm/aaaa HH:mm"); local = "dev".
val versaoNome = (findProperty("versaoNome") as String?) ?: "0.0.0"
val versaoData = (findProperty("versaoData") as String?) ?: "dev"
val (maior, menor, patch) = versaoNome.split(".").map { it.toInt() }

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.renantalk"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.renantalk"
        minSdk = 26
        targetSdk = 35
        versionCode = maxOf(1, maior * 10000 + menor * 100 + patch)
        versionName = versaoNome
        buildConfigField("String", "VERSAO_DATA", "\"$versaoData\"")
    }

    // Keystore só existe no CI (secret) e na máquina do Renan; sem ele o release sai sem assinatura.
    val keystore = System.getenv("ANDROID_KEYSTORE")
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_SENHA")
                keyAlias = System.getenv("ANDROID_CHAVE_ALIAS")
                keyPassword = System.getenv("ANDROID_CHAVE_SENHA")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            if (keystore != null) signingConfig = signingConfigs.getByName("release")
        }
        debug {
            applicationIdSuffix = ".dev"
        }
    }

    buildFeatures { buildConfig = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
