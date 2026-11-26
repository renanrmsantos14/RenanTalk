package com.renantalk

import android.content.Context
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.os.SystemClock
import android.view.View
import kotlin.math.pow

/**
 * Linha de luz do painel de voz, igual à do desktop (app/luz.py): núcleo fino e claro com brilho em volta.
 * REC = cresce do centro e o brilho segue a voz; PROC = pulso saindo do centro para as pontas;
 * ERRO = linha vermelha; PARADA = apaga em 0,5 s.
 */
class OndaView(contexto: Context) : View(contexto) {
    enum class Estado { PARADA, REC, PROC, ERRO }

    var cor = 0xFF3B82F6.toInt()
        set(v) { if (field != v) { field = v; shaders = null } }
    var corErro = 0xFFF87171.toInt()
        set(v) { if (field != v) { field = v; shaders = null } }
    private var estado = Estado.PARADA
    private var anterior = Estado.PARADA
    private var t0 = 0L
    @Volatile private var alvo = 0f
    var nivel = 0f
        private set
    private val dp = resources.displayMetrics.density
    private val tinta = Paint(Paint.ANTI_ALIAS_FLAG)
    private var shaders: Pair<Shader, Shader>? = null // núcleo, brilho
    private var shadersErro: Pair<Shader, Shader>? = null

    init { setLayerType(LAYER_TYPE_SOFTWARE, null) } // BlurMaskFilter só funciona em software

    fun definir(novo: Estado) {
        if (novo == estado) return
        anterior = estado
        estado = novo
        t0 = SystemClock.uptimeMillis()
        if (novo == Estado.REC) nivel = 0f
        postInvalidateOnAnimation()
    }

    /** Chamado da thread de gravação (0..1). */
    fun audio(n: Float) { alvo = n }

    override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) { shaders = null; shadersErro = null }

    /** Gradiente horizontal: forte no centro, apagando nas pontas ((1 - fx)^1.8, como no desktop). */
    private fun gradiente(centro: Int, ponta: Int): Shader {
        val pos = floatArrayOf(0f, 0.125f, 0.25f, 0.375f, 0.5f, 0.625f, 0.75f, 0.875f, 1f)
        val cores = pos.map { p ->
            val fx = kotlin.math.abs(p - 0.5f) * 2f
            val a = (1f - fx).pow(1.8f)
            val c = misturar(ponta, centro, 1f - fx)
            Color.argb((a * 255).toInt(), Color.red(c), Color.green(c), Color.blue(c))
        }.toIntArray()
        return LinearGradient(0f, 0f, width.toFloat(), 0f, cores, pos, Shader.TileMode.CLAMP)
    }

    private fun par(base: Int): Pair<Shader, Shader> =
        gradiente(misturar(base, Color.WHITE, 0.45f), base) to gradiente(base, misturar(base, Color.BLACK, 0.2f))

    override fun onDraw(canvas: Canvas) {
        if (width == 0) return
        val t = (SystemClock.uptimeMillis() - t0) / 1000f
        nivel += (alvo - nivel) * (if (alvo > nivel) 0.5f else 0.12f)
        val erro = estado == Estado.ERRO || (estado == Estado.PARADA && anterior == Estado.ERRO)
        val (nucleo, brilho) = if (erro) shadersErro ?: par(corErro).also { shadersErro = it }
                               else shaders ?: par(cor).also { shaders = it }
        var ganho = 1f
        var meia = width / 2f // metade visível da linha
        val n: Float
        when (estado) {
            Estado.REC -> { meia *= 1f - (1f - minOf(t / 0.45f, 1f)).pow(3); n = nivel }
            Estado.PROC -> n = 0.25f
            Estado.ERRO -> n = 0.8f
            Estado.PARADA -> {
                ganho = maxOf(0f, 1f - t / 0.5f)
                if (ganho == 0f) return
                n = if (anterior == Estado.ERRO) 0.8f else 0.3f
            }
        }
        val cx = width / 2f
        val cy = height / 2f
        canvas.save()
        canvas.clipRect(cx - meia, 0f, cx + meia, height.toFloat())

        // névoa larga e brilho que engrossa com a voz
        tinta.shader = brilho
        tinta.maskFilter = BlurMaskFilter(14 * dp, BlurMaskFilter.Blur.NORMAL)
        tinta.alpha = (255 * 0.25f * n * 2f * ganho).toInt().coerceIn(0, 255)
        if (tinta.alpha > 0) canvas.drawRect(0f, cy - 4 * dp, width.toFloat(), cy + 4 * dp, tinta)
        val raio = (1.5f + n * 6f) * dp
        tinta.maskFilter = BlurMaskFilter(raio, BlurMaskFilter.Blur.NORMAL)
        tinta.alpha = (255 * (0.45f + 0.45f * n) * ganho).toInt().coerceIn(0, 255)
        canvas.drawRect(0f, cy - raio / 2, width.toFloat(), cy + raio / 2, tinta)

        // núcleo de 2 dp, engrossa com a voz
        tinta.shader = nucleo
        tinta.maskFilter = null
        tinta.alpha = (255 * ganho).toInt()
        val esp = (1f + n * 1.5f) * dp
        canvas.drawRect(0f, cy - esp, width.toFloat(), cy + esp, tinta)

        // pulso saindo do centro ao transcrever
        if (estado == Estado.PROC) {
            val onda = t % 1f
            tinta.shader = null
            tinta.color = misturar(cor, Color.WHITE, 0.6f)
            tinta.maskFilter = BlurMaskFilter(6 * dp, BlurMaskFilter.Blur.NORMAL)
            tinta.alpha = (255 * (1f - onda)).toInt()
            val l = 28 * dp
            for (x in floatArrayOf(cx - onda * cx, cx + onda * cx))
                canvas.drawRect(x - l / 2, cy - 1.5f * dp, x + l / 2, cy + 1.5f * dp, tinta)
        }
        canvas.restore()
        postInvalidateOnAnimation()
    }

    private fun misturar(a: Int, b: Int, f: Float): Int = Color.rgb(
        (Color.red(a) + (Color.red(b) - Color.red(a)) * f).toInt(),
        (Color.green(a) + (Color.green(b) - Color.green(a)) * f).toInt(),
        (Color.blue(a) + (Color.blue(b) - Color.blue(a)) * f).toInt(),
    )
}
