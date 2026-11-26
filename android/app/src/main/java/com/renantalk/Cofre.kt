package com.renantalk

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Guarda a chave Groq cifrada (AES-GCM). A chave AES fica no Android Keystore e nunca sai do aparelho;
 * em SharedPreferences só vai o texto cifrado. allowBackup=false no manifesto: nada vai para backup.
 */
class Cofre(contexto: Context) {
    private val prefs = contexto.getSharedPreferences("cofre", Context.MODE_PRIVATE)

    fun ler(): String {
        val salvo = prefs.getString(CAMPO, null) ?: return ""
        return try {
            val bytes = Base64.decode(salvo, Base64.NO_WRAP)
            val cifra = Cipher.getInstance(TRANSFORMACAO)
            cifra.init(Cipher.DECRYPT_MODE, chaveAes(), GCMParameterSpec(128, bytes, 0, IV))
            String(cifra.doFinal(bytes, IV, bytes.size - IV), Charsets.UTF_8)
        } catch (e: Exception) {
            "" // chave do Keystore perdida (ex.: restauração): pede a chave Groq de novo
        }
    }

    fun salvar(chave: String) {
        val cifra = Cipher.getInstance(TRANSFORMACAO)
        cifra.init(Cipher.ENCRYPT_MODE, chaveAes())
        val dados = cifra.iv + cifra.doFinal(chave.toByteArray(Charsets.UTF_8))
        prefs.edit().putString(CAMPO, Base64.encodeToString(dados, Base64.NO_WRAP)).apply()
    }

    private fun chaveAes(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as SecretKey?)?.let { return it }
        val gerador = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gerador.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gerador.generateKey()
    }

    private companion object {
        const val ALIAS = "renantalk-chave-groq"
        const val CAMPO = "groq"
        const val TRANSFORMACAO = "AES/GCM/NoPadding"
        const val IV = 12
    }
}
