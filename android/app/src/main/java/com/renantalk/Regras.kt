package com.renantalk

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.sqrt

/** Regras do ditado, iguais às de app/ditado.pyw e app/comum.py (sem dependência de Android, para testar). */
object Regras {
    const val TAXA = 16000
    const val MINIMO_S = 0.4 // gravações mais curtas são ignoradas (toque acidental)
    const val SILENCIO = 800 // pico abaixo disso (de 32767) é tratado como silêncio
    const val LIMITE_VOCAB = 850 // caracteres; o Whisper só aproveita ~224 tokens de contexto

    val VOCAB_PADRAO = listOf(
        "Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI",
        "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp",
    )

    enum class Decisao { CURTO, SILENCIO, ENVIAR }

    /** O Whisper devolve "." ou "…" quando só ouve ruído: sem letra nem número, não há fala. */
    fun semFala(texto: String) = texto.none { it.isLetterOrDigit() }

    /** pcm = PCM 16 bits little-endian mono. */
    fun decidir(pcm: ByteArray, duracaoS: Double): Decisao {
        if (duracaoS < MINIMO_S) return Decisao.CURTO
        return if (pico(pcm) < SILENCIO) Decisao.SILENCIO else Decisao.ENVIAR // o Whisper inventa texto no silêncio
    }

    fun pico(pcm: ByteArray): Int {
        val b = ByteBuffer.wrap(pcm, 0, pcm.size - pcm.size % 2).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        var max = 0
        while (b.hasRemaining()) max = maxOf(max, abs(b.get().toInt()))
        return max
    }

    /** Brilho da luz (0..1) a partir do RMS das amostras. */
    fun nivel(amostras: ShortArray, n: Int): Float {
        if (n <= 0) return 0f
        var soma = 0.0
        for (i in 0 until n) soma += amostras[i].toDouble() * amostras[i]
        val rms = sqrt(soma / n)
        if (rms < 1) return 0f
        // escala em dB, como o ouvido: ruído (~-55 dBFS) = 0, voz alta (~-17 dBFS) = 1
        val db = 20 * log10(rms / 32768)
        return ((db + 55) / 38).coerceIn(0.0, 1.0).toFloat()
    }

    /** Termos separados por vírgula, cortando no limite sem quebrar um termo ao meio. */
    fun promptVocabulario(termos: List<String>): String {
        val texto = StringBuilder()
        for (termo in termos) {
            if (texto.length + termo.length + 2 > LIMITE_VOCAB) break
            if (texto.isNotEmpty()) texto.append(", ")
            texto.append(termo)
        }
        return if (texto.isEmpty()) "" else "$texto."
    }

    /** Quantos termos (do início) cabem no prompt; os demais ficam de fora. */
    fun termosQueCabem(termos: List<String>): Int {
        var tamanho = 0
        termos.forEachIndexed { i, termo ->
            if (tamanho + termo.length + 2 > LIMITE_VOCAB) return i
            tamanho += termo.length + if (i == 0) 0 else 2
        }
        return termos.size
    }

    fun wav(pcm: ByteArray, taxa: Int = TAXA): ByteArray {
        val cab = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
        cab.put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVE".toByteArray())
        cab.put("fmt ".toByteArray()).putInt(16).putShort(1).putShort(1).putInt(taxa).putInt(taxa * 2)
            .putShort(2).putShort(16)
        cab.put("data".toByteArray()).putInt(pcm.size)
        return ByteArrayOutputStream(44 + pcm.size).apply { write(cab.array()); write(pcm) }.toByteArray()
    }

    /** "0.0.12" > "0.0.9"; aceita prefixo "v". */
    fun versaoMaior(nova: String, atual: String): Boolean {
        fun partes(v: String) = v.removePrefix("v").split(".").map { it.toIntOrNull() ?: 0 }
        val a = partes(nova)
        val b = partes(atual)
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }

    /**
     * Texto do campo depois de inserir o ditado no cursor (ou no lugar da seleção), com espaço
     * antes quando o caractere anterior não for espaço. Devolve o texto novo e a posição do cursor.
     */
    fun inserir(atual: String, ini: Int, fim: Int, ditado: String): Pair<String, Int> {
        val a = if (ini in 0..atual.length) ini else atual.length
        val b = if (fim in a..atual.length) fim else a
        val antes = atual.substring(0, a)
        val espaco = if (antes.isNotEmpty() && !antes.last().isWhitespace()) " " else ""
        val novo = antes + espaco + ditado + atual.substring(b)
        return novo to (a + espaco.length + ditado.length)
    }
}
