package com.renantalk

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.os.SystemClock
import android.view.View

/**
 * Linha de luz azul no topo do teclado, a mesma identidade da luz do desktop (app/luz.py):
 * REC nasce no centro e o brilho segue a voz; PROC pulsa do centro para as pontas;
 * FEITO apaga em 0,5 s; ERRO fica vermelha e apaga.
 */
class LuzView(contexto: Context) : View(contexto) {
    enum class Estado { OCIOSO, REC, PROC, FEITO, ERRO }

    private var estado = Estado.OCIOSO
    private var desde = 0L
    @Volatile private var alvo = 0f
    private var brilho = 0f
    private val tinta = Paint(Paint.ANTI_ALIAS_FLAG)
    private val densidade = resources.displayMetrics.density

    fun definir(novo: Estado) {
        estado = novo
        desde = SystemClock.uptimeMillis()
        if (novo == Estado.REC) brilho = 0f
        postInvalidateOnAnimation()
    }

    /** Chamado da thread de gravação (0..1). */
    fun audio(nivel: Float) {
        alvo = nivel
    }

    override fun onDraw(canvas: Canvas) {
        val t = (SystemClock.uptimeMillis() - desde).toFloat()
        val w = width.toFloat()
        val meio = w / 2
        val y = height / 2f
        when (estado) {
            Estado.OCIOSO -> return
            Estado.REC -> {
                brilho += (alvo - brilho) * 0.3f
                val abertura = minOf(1f, t / 250f) // cresce do centro em 250 ms
                linha(canvas, meio, meio * abertura, y, 0.45f + 0.55f * brilho, AZUL, AZUL_CLARO)
            }
            Estado.PROC -> {
                linha(canvas, meio, meio, y, 0.35f, AZUL, AZUL_CLARO)
                val p = (t % 900f) / 900f // pulso saindo do centro a cada 0,9 s
                linha(canvas, meio + meio * p, meio * 0.25f, y, 1f - p * 0.6f, AZUL, AZUL_CLARO)
                linha(canvas, meio - meio * p, meio * 0.25f, y, 1f - p * 0.6f, AZUL, AZUL_CLARO)
            }
            Estado.FEITO, Estado.ERRO -> {
                val duracao = if (estado == Estado.ERRO) 1200f else 500f
                if (t >= duracao) {
                    estado = Estado.OCIOSO
                    invalidate()
                    return
                }
                val alfa = 1f - t / duracao
                if (estado == Estado.ERRO) linha(canvas, meio, meio, y, alfa, VERMELHO, VERMELHO_CLARO)
                else linha(canvas, meio, meio, y, alfa * 0.8f, AZUL, AZUL_CLARO)
            }
        }
        postInvalidateOnAnimation()
    }

    /** Segmento centrado em cx, meia largura r: brilho no centro, some nas pontas. */
    private fun linha(canvas: Canvas, cx: Float, r: Float, y: Float, alfa: Float, ponta: Int, centro: Int) {
        if (r <= 0f) return
        val a = (alfa.coerceIn(0f, 1f) * 255).toInt()
        val cores = intArrayOf(Color.TRANSPARENT, comAlfa(ponta, a), comAlfa(centro, a), comAlfa(ponta, a), Color.TRANSPARENT)
        val pos = floatArrayOf(0f, 0.2f, 0.5f, 0.8f, 1f)
        tinta.shader = LinearGradient(cx - r, 0f, cx + r, 0f, cores, pos, Shader.TileMode.CLAMP)
        tinta.strokeCap = Paint.Cap.ROUND
        tinta.strokeWidth = height * 0.9f // névoa
        tinta.alpha = 70
        canvas.drawLine(cx - r, y, cx + r, y, tinta)
        tinta.strokeWidth = 2.5f * densidade // núcleo
        tinta.alpha = 255
        canvas.drawLine(cx - r, y, cx + r, y, tinta)
    }

    private fun comAlfa(cor: Int, a: Int) = (cor and 0x00FFFFFF) or (a shl 24)

    private companion object {
        val AZUL = Color.rgb(37, 99, 235)
        val AZUL_CLARO = Color.rgb(160, 196, 255)
        val VERMELHO = Color.rgb(220, 38, 38)
        val VERMELHO_CLARO = Color.rgb(254, 202, 202)
    }
}
