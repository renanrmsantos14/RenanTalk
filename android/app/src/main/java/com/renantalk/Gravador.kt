package com.renantalk

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.SystemClock
import java.io.ByteArrayOutputStream

/** Grava PCM 16 kHz mono 16 bits em memória (nada vai para o disco) e informa o nível da voz. */
class Gravador(private val aoNivel: (Float) -> Unit) {
    class Gravacao(val pcm: ByteArray, val duracaoS: Double)

    @Volatile private var gravando = false
    private var thread: Thread? = null
    private var buffer = ByteArrayOutputStream()
    private var inicio = 0L

    val ativo get() = gravando

    /** Precisa da permissão RECORD_AUDIO (concedida na tela de configurações). */
    @SuppressLint("MissingPermission")
    fun comecar() {
        if (gravando) return
        val minimo = AudioRecord.getMinBufferSize(Regras.TAXA, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val rec = AudioRecord(
            MediaRecorder.AudioSource.VOICE_RECOGNITION, Regras.TAXA,
            AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(minimo, Regras.TAXA / 5 * 2),
        )
        if (rec.state != AudioRecord.STATE_INITIALIZED) {
            rec.release()
            throw IllegalStateException("Microfone indisponível.")
        }
        buffer = ByteArrayOutputStream(Regras.TAXA * 2 * 10)
        gravando = true
        inicio = SystemClock.elapsedRealtime()
        rec.startRecording()
        thread = Thread {
            val amostras = ShortArray(Regras.TAXA / 20) // 50 ms
            val bytes = ByteArray(amostras.size * 2)
            try {
                while (gravando) {
                    val n = rec.read(amostras, 0, amostras.size)
                    if (n <= 0) continue
                    for (i in 0 until n) {
                        bytes[2 * i] = (amostras[i].toInt() and 0xFF).toByte()
                        bytes[2 * i + 1] = (amostras[i].toInt() shr 8).toByte()
                    }
                    buffer.write(bytes, 0, n * 2)
                    aoNivel(Regras.nivel(amostras, n))
                }
            } finally {
                rec.stop()
                rec.release()
            }
        }.apply { name = "gravador"; start() }
    }

    fun parar(): Gravacao {
        gravando = false
        thread?.join(1000)
        thread = null
        return Gravacao(buffer.toByteArray(), (SystemClock.elapsedRealtime() - inicio) / 1000.0)
    }
}
