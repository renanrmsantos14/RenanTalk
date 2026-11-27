package com.renantalk

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.SweepGradient
import android.os.Build
import android.os.SystemClock
import android.view.RoundedCorner
import android.view.View
import kotlin.math.abs
import kotlin.math.pow

/**
 * A linha de luz do desktop (app/luz.py), mas em volta da tela inteira: um núcleo fino colado na borda
 * e o brilho saindo só para dentro (a outra metade fica fora da tela), mais forte embaixo no centro,
 * de onde nasce, e mais fraca em cima.
 * REC = abre pelos dois lados em 0,45 s e o brilho segue a voz; PROC = pulso saindo de baixo e subindo
 * pelos lados; FIM = apaga em 0,5 s; ERRO = vermelha e apaga; PARADA = apaga rápido.
 */
class ContornoView(contexto: Context) : View(contexto) {
    enum class Estado { PARADA, REC, PROC, FIM, ERRO }

    /** Chamado quando a luz terminou de apagar (a janela pode sair). */
    var aoApagar: (() -> Unit)? = null

    private val dp = resources.displayMetrics.density
    private var estado = Estado.PARADA
    private var t0 = 0L
    private var apagou = true
    @Volatile private var alvo = 0f
    private var nivel = 0f
    private var nAntes = 0.3f // intensidade no momento em que começou a apagar

    private val caminho = Path()
    private val trecho = Path()
    private val medida = PathMeasure()
    private var comprimento = 0f
    private var azul: Array<Shader>? = null // núcleo, brilho, névoa
    private var vermelho: Array<Shader>? = null
    private val tinta = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private val nevoa = BlurMaskFilter(12 * dp, BlurMaskFilter.Blur.NORMAL)
    // o brilho "abre" com a voz (raio 1,5 a 7,5 dp, como 1.5 + n*6 px no desktop): um filtro por degrau
    private val brilhos = Array(11) { BlurMaskFilter((1.5f + 6f * it / 10) * dp, BlurMaskFilter.Blur.NORMAL) }

    init {
        // BlurMaskFilter só tem aceleração por hardware a partir do Android 9
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) setLayerType(LAYER_TYPE_SOFTWARE, null)
    }

    fun definir(novo: Estado) {
        if (novo == estado && novo != Estado.ERRO) return
        nAntes = intensidade()
        estado = novo
        t0 = SystemClock.uptimeMillis()
        apagou = false
        if (novo == Estado.REC) nivel = 0f
        postInvalidateOnAnimation()
    }

    /** Chamado da thread de gravação (0..1). */
    fun audio(n: Float) { alvo = n }

    override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) = montar(w.toFloat(), h.toFloat())

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (width > 0) montar(width.toFloat(), height.toFloat())
    }

    /** Raio real do canto da tela (Android 12+); antes disso, um valor comum de 28 dp. */
    private fun raio(topo: Boolean): Float {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return 28 * dp
        val pos = if (topo) RoundedCorner.POSITION_TOP_LEFT else RoundedCorner.POSITION_BOTTOM_LEFT
        return display?.getRoundedCorner(pos)?.radius?.toFloat() ?: 0f
    }

    /** Borda exata da tela, começando embaixo no centro (de onde a luz nasce). */
    private fun montar(w: Float, h: Float) {
        val rc = raio(topo = true)
        val rb = raio(topo = false)
        caminho.reset()
        caminho.moveTo(w / 2, h)
        caminho.lineTo(w - rb, h)
        if (rb > 0) caminho.arcTo(RectF(w - 2 * rb, h - 2 * rb, w, h), 90f, -90f)
        caminho.lineTo(w, rc)
        if (rc > 0) caminho.arcTo(RectF(w - 2 * rc, 0f, w, 2 * rc), 0f, -90f)
        caminho.lineTo(rc, 0f)
        if (rc > 0) caminho.arcTo(RectF(0f, 0f, 2 * rc, 2 * rc), 270f, -90f)
        caminho.lineTo(0f, h - rb)
        if (rb > 0) caminho.arcTo(RectF(0f, h - 2 * rb, 2 * rb, h), 180f, -90f)
        caminho.close()
        medida.setPath(caminho, false)
        comprimento = medida.length
        // cores do desktop: CENTRO 120,170,255 · PONTA 37,99,235 · BRILHO 45,110,255 · NÉVOA 30,64,200
        azul = arrayOf(gradiente(w, h, 0xFF78AAFF.toInt(), 0xFF2563EB.toInt()),
            gradiente(w, h, 0xFF2D6EFF.toInt(), 0xFF2D6EFF.toInt()), gradiente(w, h, 0xFF1E40C8.toInt(), 0xFF1E40C8.toInt()))
        vermelho = arrayOf(gradiente(w, h, 0xFFFECACA.toInt(), 0xFFFECACA.toInt()),
            gradiente(w, h, 0xFFF87171.toInt(), 0xFFF87171.toInt()), gradiente(w, h, 0xFFDC2626.toInt(), 0xFFDC2626.toInt()))
    }

    /**
     * Gradiente em volta do centro da tela: cor e força máximas embaixo (90°) e caindo até em cima,
     * como o (1 - fx)^1.8 do desktop, mas sem zerar: a tela toda fica contornada.
     */
    private fun gradiente(w: Float, h: Float, centro: Int, ponta: Int): Shader {
        val passos = 24
        val pos = FloatArray(passos + 1) { it / passos.toFloat() }
        val cores = IntArray(passos + 1) { i ->
            val graus = pos[i] * 360f
            val fx = minOf(abs(graus - 90f), 360f - abs(graus - 90f)) / 180f // 0 embaixo, 1 em cima
            val a = 0.3f + 0.7f * (1f - fx).pow(1.8f)
            val c = misturar(ponta, centro, 1f - fx)
            Color.argb((a * 255).toInt(), Color.red(c), Color.green(c), Color.blue(c))
        }
        return SweepGradient(w / 2, h / 2, cores, pos)
    }

    private fun intensidade(): Float = when (estado) {
        Estado.REC -> nivel
        Estado.PROC -> 0.25f
        Estado.ERRO -> 0.8f
        else -> 0.3f
    }

    /** Liga um trecho [de, ate] do contorno em `trecho` (dá a volta se passar do fim). */
    private fun pegar(de: Float, ate: Float) {
        val a = ((de % comprimento) + comprimento) % comprimento
        val b = a + (ate - de)
        medida.getSegment(a, minOf(b, comprimento), trecho, true)
        if (b > comprimento) medida.getSegment(0f, b - comprimento, trecho, true)
    }

    override fun onDraw(canvas: Canvas) {
        if (comprimento == 0f || apagou) return
        val t = (SystemClock.uptimeMillis() - t0) / 1000f
        val animar = ValueAnimator.areAnimatorsEnabled()
        nivel += (alvo - nivel) * (if (alvo > nivel) 0.5f else 0.12f)

        var ganho = 1f
        var aberto = 1f
        when (estado) {
            Estado.REC -> if (animar) aberto = 1f - (1f - minOf(t / 0.45f, 1f)).pow(3)
            Estado.PROC -> Unit
            Estado.ERRO -> ganho = minOf(1f, t / 0.12f) * (if (t > 0.7f) 1f - (t - 0.7f) / 0.5f else 1f)
            Estado.FIM -> ganho = 1f - t / 0.5f
            Estado.PARADA -> ganho = 1f - t / 0.3f
        }
        val n = if (estado == Estado.PARADA) nAntes else intensidade()
        if (ganho <= 0f) { apagou = true; aoApagar?.invoke(); return }
        val (cNucleo, cBrilho, cNevoa) = (if (estado == Estado.ERRO) vermelho else azul) ?: return

        trecho.reset()
        if (aberto >= 1f) trecho.set(caminho) else pegar(-aberto * comprimento / 2, aberto * comprimento / 2)

        // a linha fica em cima da borda: só a metade de dentro aparece, então as larguras são o dobro
        fun traco(shader: Shader, largura: Float, filtro: BlurMaskFilter?, a: Float) {
            tinta.shader = shader
            tinta.strokeWidth = largura
            tinta.maskFilter = filtro
            tinta.alpha = (255 * a * ganho).toInt().coerceIn(0, 255)
            if (tinta.alpha > 0) canvas.drawPath(trecho, tinta)
        }
        traco(cNevoa, 8 * dp, nevoa, 0.25f * n * 2f)
        traco(cBrilho, (2f + n * 4f) * dp, brilhos[(n * 10).toInt().coerceIn(0, 10)], 0.45f + 0.45f * n)
        traco(cNucleo, 2.4f * dp, null, if (estado == Estado.PROC) 0.6f else 1f)

        // pulso saindo de baixo e subindo pelos dois lados, uma vez por segundo
        if (estado == Estado.PROC && animar) {
            val d = (t % 1f) * comprimento / 2
            val l = 70 * dp
            trecho.reset()
            pegar(d - l / 2, d + l / 2)
            pegar(-d - l / 2, -d + l / 2)
            traco(cBrilho, 6 * dp, brilhos[6], 0.55f)
            traco(cNucleo, 2.4f * dp, null, 0.9f)
        }
        tinta.maskFilter = null
        postInvalidateOnAnimation()
    }

    private fun misturar(a: Int, b: Int, f: Float): Int = Color.rgb(
        (Color.red(a) + (Color.red(b) - Color.red(a)) * f).toInt(),
        (Color.green(a) + (Color.green(b) - Color.green(a)) * f).toInt(),
        (Color.blue(a) + (Color.blue(b) - Color.blue(a)) * f).toInt(),
    )
}
