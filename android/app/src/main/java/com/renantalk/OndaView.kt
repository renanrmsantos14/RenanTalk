package com.renantalk

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.os.SystemClock
import android.view.View
import kotlin.math.abs
import kotlin.math.sin

/**
 * Onda de barras do painel de voz (herdeira da linha de luz do desktop):
 * REC = barras saem do centro seguindo a voz; PROC = onda que corre de um lado ao outro;
 * ERRO = barras baixas na cor de erro; PARADA = barras mínimas e apagadas.
 */
class OndaView(contexto: Context) : View(contexto) {
    enum class Estado { PARADA, REC, PROC, ERRO }

    var cor = 0xFF3B82F6.toInt()
    var corErro = 0xFFF87171.toInt()
    private var estado = Estado.PARADA
    @Volatile private var alvo = 0f
    var nivel = 0f
        private set
    private val historico = FloatArray(BARRAS / 2 + 1)
    private val tinta = Paint(Paint.ANTI_ALIAS_FLAG).apply { strokeCap = Paint.Cap.ROUND }
    private val densidade = resources.displayMetrics.density

    fun definir(novo: Estado) {
        estado = novo
        if (novo == Estado.REC) { historico.fill(0f); nivel = 0f }
        postInvalidateOnAnimation()
    }

    /** Chamado da thread de gravação (0..1). */
    fun audio(n: Float) { alvo = n }

    override fun onDraw(canvas: Canvas) {
        val t = SystemClock.uptimeMillis() / 1000f
        val meio = BARRAS / 2
        if (estado == Estado.REC) {
            nivel += (alvo - nivel) * 0.35f
            // a voz nasce no centro e se espalha para as pontas
            for (i in historico.size - 1 downTo 1) historico[i] = historico[i - 1] * 0.96f
            historico[0] = nivel
        }
        val passo = width.toFloat() / BARRAS
        tinta.strokeWidth = minOf(passo * 0.55f, 4.5f * densidade)
        val cy = height / 2f
        val maximo = height / 2f - tinta.strokeWidth
        for (i in 0 until BARRAS) {
            val d = abs(i - meio)
            val forma = 1f - d.toFloat() / (meio + 1) * 0.55f // mais alta no centro
            val a = when (estado) {
                Estado.REC -> 0.12f + 0.88f * historico[minOf(d, historico.size - 1)] * forma
                Estado.PROC -> 0.18f + 0.5f * ((sin(t * 7f - i * 0.45f) + 1f) / 2f) * forma
                Estado.ERRO -> 0.15f * forma
                Estado.PARADA -> 0.08f
            }
            val h = maxOf(0.5f, a * maximo)
            val x = passo * (i + 0.5f)
            tinta.color = if (estado == Estado.ERRO) corErro else cor
            tinta.alpha = when (estado) {
                Estado.PARADA -> 90
                Estado.REC -> (140 + 115 * forma).toInt().coerceAtMost(255)
                else -> 220
            }
            canvas.drawLine(x, cy - h, x, cy + h, tinta)
        }
        if (estado == Estado.REC || estado == Estado.PROC) postInvalidateOnAnimation()
    }

    private companion object { const val BARRAS = 31 }
}
