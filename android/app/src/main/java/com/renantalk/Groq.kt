package com.renantalk

import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID

/** Transcrição na Groq (Whisper large-v3), mesmos campos de app/ditado.pyw. */
object Groq {
    private const val URL_TRANSCRICAO = "https://api.groq.com/openai/v1/audio/transcriptions"
    private const val URL_MODELOS = "https://api.groq.com/openai/v1/models"
    private const val MODELO = "whisper-large-v3"
    private val AGENTE = "renantalk-android/${BuildConfig.VERSION_NAME}"

    class Erro(val codigo: Int, mensagem: String) : IOException(mensagem)

    fun transcrever(chave: String, wav: ByteArray, prompt: String): String {
        val limite = UUID.randomUUID().toString().replace("-", "")
        val campos = linkedMapOf("model" to MODELO, "language" to "pt", "temperature" to "0", "response_format" to "json")
        if (prompt.isNotEmpty()) campos["prompt"] = prompt // contexto (não ordem) para grafar certo os termos
        val corpo = ByteArrayOutputStream(wav.size + 2048)
        for ((nome, valor) in campos) {
            corpo.write("--$limite\r\nContent-Disposition: form-data; name=\"$nome\"\r\n\r\n$valor\r\n".toByteArray())
        }
        corpo.write(
            ("--$limite\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n" +
                "Content-Type: audio/wav\r\n\r\n").toByteArray()
        )
        corpo.write(wav)
        corpo.write("\r\n--$limite--\r\n".toByteArray())

        val con = (URL(URL_TRANSCRICAO).openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            doOutput = true
            connectTimeout = 10_000
            readTimeout = 30_000
            setRequestProperty("Authorization", "Bearer $chave")
            setRequestProperty("Content-Type", "multipart/form-data; boundary=$limite")
            setRequestProperty("User-Agent", AGENTE)
            setFixedLengthStreamingMode(corpo.size())
        }
        try {
            con.outputStream.use { corpo.writeTo(it) }
            val codigo = con.responseCode
            if (codigo != 200) throw Erro(codigo, mensagemDeErro(codigo))
            val json = con.inputStream.bufferedReader().use { it.readText() }
            return JSONObject(json).getString("text").trim()
        } finally {
            con.disconnect()
        }
    }

    /** null se a Groq aceitar a chave; senão, a mensagem para mostrar. */
    fun validar(chave: String): String? {
        val con = (URL(URL_MODELOS).openConnection() as HttpURLConnection).apply {
            connectTimeout = 10_000
            readTimeout = 15_000
            setRequestProperty("Authorization", "Bearer $chave")
            setRequestProperty("User-Agent", AGENTE)
        }
        return try {
            when (val codigo = con.responseCode) {
                200 -> null
                401, 403 -> "A Groq recusou essa chave."
                else -> "A Groq respondeu erro $codigo."
            }
        } catch (e: IOException) {
            "Sem conexão com a Groq. Tente de novo."
        } finally {
            con.disconnect()
        }
    }

    fun mensagemDeErro(codigo: Int) = when (codigo) {
        401, 403 -> "Chave Groq inválida. Troque nas configurações."
        429 -> "Limite da Groq atingido. Espere um pouco."
        else -> "A Groq respondeu erro $codigo."
    }
}
