package com.renantalk

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import android.graphics.RectF
import android.graphics.Typeface
import android.os.SystemClock
import android.view.View
import kotlin.math.sqrt

/**
 * A pílula sobre o teclado, no estilo "ilha dinâmica": parada mostra o microfone e "Ditar";
 * gravando, cresce com mola e vira cancelar ✕ + ondas da voz + tempo + enviar ↑;
 * escrevendo, encolhe num círculo com um anel girando; no fim mostra ✓ e volta a ser a pílula.
 * As bordas da cápsula seguem uma mola (posição e velocidade), então trocar de estado no meio
 * da animação continua suave.
 */
class IlhaView(contexto: Context) : View(contexto) {
    enum class Modo { DITAR, GRAVANDO, ESCREVENDO, OK }

    /** Chamado quando a cápsula voltou ao tamanho da pílula (a janela pode encolher). */
    var aoRecolher: (() -> Unit)? = null
    /** Quando a gravação começou (para o relógio). */
    var inicio = 0L
    /** x da pílula dentro desta view, quando a janela está aberta e a pílula não está no canto. */
    var ancora = 0f

    private val dp = resources.displayMetrics.density * ESCALA // tudo da ilha cresce junto
    private val fonte: Typeface = resources.getFont(R.font.geist_semibold)
    var modo = Modo.DITAR
        private set
    private var trocouEm = 0L
    @Volatile private var alvo = 0f
    private var nivel = 0f
    private val historico = FloatArray(40)
    private var amostrouEm = 0L
    private var quadroEm = 0L

    // mola das bordas esquerda e direita da cápsula
    private var esq = 0f; private var dir = 0f
    private var vEsq = 0f; private var vDir = 0f
    private var repousou = true

    private val icone = contexto.getDrawable(R.drawable.ic_mic)!!.mutate()
    private val fundo = Paint(Paint.ANTI_ALIAS_FLAG)
    private val borda = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val texto = Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = fonte; textSize = 12.5f * resources.displayMetrics.scaledDensity * ESCALA }
    private val relogio = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = fonte; textSize = 11.5f * resources.displayMetrics.scaledDensity * ESCALA
        fontFeatureSettings = "tnum"; color = 0xFF94A3B8.toInt(); textAlign = Paint.Align.RIGHT
    }
    private val traco = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
    private val cheio = Paint(Paint.ANTI_ALIAS_FLAG)
    private val capsula = Path()
    private val rect = RectF()
    private val visto = Path()
    private val vistoParcial = Path()
    private val vistoMedida = PathMeasure()

    val margem = (10 * dp).toInt() // espaço da sombra em volta da cápsula
    val altura = (30 * dp).toInt()
    /** Largura da janela com a pílula parada. */
    val pilulaL = margem * 2 + (10 * dp + 14 * dp + 5 * dp + texto.measureText("Ditar") + 12 * dp).toInt()
    /** Largura da janela com a ilha aberta. */
    val abertaL = margem * 2 + (196 * dp).toInt()
    val janelaA = altura + margem * 2

    init {
        fundo.setShadowLayer(10 * dp, 0f, 3 * dp, 0x99000000.toInt())
        setLayerType(LAYER_TYPE_HARDWARE, null)
    }

    fun audio(n: Float) { alvo = n }

    fun definir(novo: Modo) {
        if (novo == modo) return
        modo = novo
        trocouEm = SystemClock.uptimeMillis()
        if (novo == Modo.GRAVANDO) { nivel = 0f; historico.fill(0f) }
        repousou = false
        contentDescription = when (novo) {
            Modo.DITAR -> "Ditar com o RenanTalk: segure e fale, ou toque para começar"
            Modo.GRAVANDO -> "Gravando: toque para enviar, ou no X para cancelar"
            Modo.ESCREVENDO -> "Escrevendo o texto"
            Modo.OK -> "Texto inserido"
        }
        if (novo == Modo.OK) postDelayed({ if (modo == Modo.OK) definir(Modo.DITAR) }, 650)
        postInvalidateOnAnimation()
    }

    /** Move a cápsula junto quando a janela muda de x (dx = x antigo - x novo). */
    fun deslocar(dx: Float) { esq += dx; dir += dx; invalidate() }

    /** Coloca a cápsula direto no alvo, sem animar. */
    fun repousar() {
        val (l, r) = alvoAtual()
        esq = l; dir = r; vEsq = 0f; vDir = 0f; repousou = true
        invalidate()
    }

    /** Toque na área do ✕ (só existe com a ilha aberta). */
    fun tocouCancelar(x: Float) = modo == Modo.GRAVANDO && x < esq + 36 * dp

    private fun alvoAtual(): Pair<Float, Float> = when (modo) {
        Modo.DITAR -> (ancora + margem) to (ancora + pilulaL - margem)
        Modo.GRAVANDO -> margem.toFloat() to (width - margem).toFloat()
        Modo.ESCREVENDO, Modo.OK -> (width / 2f - altura / 2f) to (width / 2f + altura / 2f)
    }

    private fun mola(dt: Float) {
        val (l, r) = alvoAtual()
        if (!ValueAnimator.areAnimatorsEnabled()) { esq = l; dir = r; vEsq = 0f; vDir = 0f }
        // rigidez 520, amortecimento 0,72 do crítico: um leve "quique" no fim
        val k = 520f; val c = 2 * sqrt(k) * 0.72f
        vEsq += (k * (l - esq) - c * vEsq) * dt; esq += vEsq * dt
        vDir += (k * (r - dir) - c * vDir) * dt; dir += vDir * dt
        val parado = kotlin.math.abs(l - esq) < 0.5f && kotlin.math.abs(r - dir) < 0.5f &&
            kotlin.math.abs(vEsq) < 8f && kotlin.math.abs(vDir) < 8f
        if (parado) { esq = l; dir = r; vEsq = 0f; vDir = 0f }
        if (parado && !repousou) {
            repousou = true
            if (modo == Modo.DITAR) post { if (modo == Modo.DITAR) aoRecolher?.invoke() }
        }
    }

    override fun onDraw(canvas: Canvas) {
        if (width == 0) return
        val agora = SystemClock.uptimeMillis()
        val dt = if (quadroEm == 0L) 0.016f else ((agora - quadroEm) / 1000f).coerceIn(0.001f, 0.033f)
        quadroEm = agora
        if (esq == 0f && dir == 0f) repousar()
        mola(dt)
        nivel += (alvo - nivel) * (if (alvo > nivel) 0.5f else 0.12f)
        if (modo == Modo.GRAVANDO && agora - amostrouEm >= 60) {
            System.arraycopy(historico, 1, historico, 0, historico.size - 1)
            historico[historico.size - 1] = nivel
            amostrouEm = agora
        }
        val t = (agora - trocouEm) / 1000f
        val aparece = ((t - 0.06f) / 0.18f).coerceIn(0f, 1f) // conteúdo entra depois da cápsula começar a mudar

        val topo = margem.toFloat()
        val base = topo + altura
        val raio = altura / 2f
        rect.set(esq, topo, maxOf(dir, esq + altura), base)
        capsula.reset()
        capsula.addRoundRect(rect, raio, raio, Path.Direction.CW)

        fundo.color = if (modo == Modo.DITAR) 0xF00F172A.toInt() else 0xF8080B12.toInt()
        canvas.drawPath(capsula, fundo)
        // contorno da cápsula: acende azul com a voz enquanto grava
        borda.strokeWidth = 1 * dp
        borda.color = if (modo == Modo.GRAVANDO) {
            val a = (0.25f + nivel * 0.6f).coerceAtMost(0.9f)
            ((a * 255).toInt() shl 24) or 0x3B82F6
        } else 0x24FFFFFF
        canvas.drawPath(capsula, borda)

        canvas.save()
        canvas.clipPath(capsula)
        val alfa = (aparece * 255).toInt()
        when (modo) {
            Modo.DITAR -> desenharDitar(canvas, alfa)
            Modo.GRAVANDO -> desenharGravando(canvas, alfa, topo, base, agora)
            Modo.ESCREVENDO -> desenharAnel(canvas, alfa, t)
            Modo.OK -> desenharOk(canvas, alfa, t)
        }
        canvas.restore()
        if (!repousou || modo != Modo.DITAR || aparece < 1f) postInvalidateOnAnimation()
    }

    private fun desenharDitar(c: Canvas, alfa: Int) {
        val cor = 0xFFDBEAFE.toInt()
        val w = 14 * dp + 5 * dp + texto.measureText("Ditar")
        var x = rect.centerX() - w / 2
        val cy = rect.centerY()
        icone.setTint(0xFF93C5FD.toInt())
        icone.alpha = alfa
        icone.setBounds(x.toInt(), (cy - 7 * dp).toInt(), (x + 14 * dp).toInt(), (cy + 7 * dp).toInt())
        icone.draw(c)
        x += 19 * dp
        texto.color = cor; texto.alpha = alfa
        c.drawText("Ditar", x, cy - (texto.ascent() + texto.descent()) / 2, texto)
    }

    private fun desenharGravando(c: Canvas, alfa: Int, topo: Float, base: Float, agora: Long) {
        val cy = (topo + base) / 2
        val r = 11.5f * dp
        // ✕ cancelar
        val xc = margem + 3.5f * dp + r
        cheio.color = 0x14FFFFFF; cheio.alpha = (0x14 * alfa / 255)
        c.drawCircle(xc, cy, r, cheio)
        traco.color = 0xFF94A3B8.toInt(); traco.alpha = alfa; traco.strokeWidth = 1.6f * dp
        val s = 3.5f * dp
        c.drawLine(xc - s, cy - s, xc + s, cy + s, traco); c.drawLine(xc - s, cy + s, xc + s, cy - s, traco)
        // ↑ enviar
        val xe = width - margem - 3.5f * dp - r
        cheio.color = 0xFF3B82F6.toInt(); cheio.alpha = alfa
        c.drawCircle(xe, cy, r * (1f + nivel * 0.08f), cheio)
        traco.color = 0xFFFFFFFF.toInt(); traco.alpha = alfa; traco.strokeWidth = 2 * dp
        c.drawLine(xe, cy + 4.5f * dp, xe, cy - 4.5f * dp, traco)
        c.drawLine(xe - 4 * dp, cy - 0.5f * dp, xe, cy - 4.5f * dp, traco)
        c.drawLine(xe + 4 * dp, cy - 0.5f * dp, xe, cy - 4.5f * dp, traco)
        // tempo
        val seg = ((agora - inicio) / 1000).toInt()
        val xt = xe - r - 8 * dp
        relogio.alpha = alfa
        c.drawText("%d:%02d".format(seg / 60, seg % 60), xt, cy - (relogio.ascent() + relogio.descent()) / 2, relogio)
        // ondas da voz: as mais novas à direita, rolando para a esquerda
        val ini = xc + r + 10 * dp
        val fim = xt - relogio.measureText("0:00") - 10 * dp
        val passo = 4.5f * dp
        val n = ((fim - ini) / passo).toInt().coerceIn(0, historico.size)
        cheio.color = 0xFFE2E8F0.toInt()
        for (k in 0 until n) {
            val v = historico[historico.size - n + k]
            val h = maxOf(2.5f * dp, v * 18 * dp)
            val x = ini + k * passo
            cheio.alpha = (alfa * (0.35f + 0.65f * k / maxOf(1, n - 1))).toInt()
            c.drawRoundRect(x, cy - h / 2, x + 2.2f * dp, cy + h / 2, 1.1f * dp, 1.1f * dp, cheio)
        }
    }

    private fun desenharAnel(c: Canvas, alfa: Int, t: Float) {
        val cx = rect.centerX(); val cy = rect.centerY(); val r = 7 * dp
        traco.strokeWidth = 2.2f * dp
        traco.color = 0x22FFFFFF; traco.alpha = 0x22 * alfa / 255
        c.drawCircle(cx, cy, r, traco)
        traco.color = 0xFF93C5FD.toInt(); traco.alpha = alfa
        rect.set(cx - r, cy - r, cx + r, cy + r)
        c.drawArc(rect, (t * 514f) % 360f, 100f, false, traco) // uma volta a cada 0,7 s
    }

    private fun desenharOk(c: Canvas, alfa: Int, t: Float) {
        val cx = rect.centerX(); val cy = rect.centerY()
        visto.reset()
        visto.moveTo(cx - 6 * dp, cy + 0.5f * dp); visto.lineTo(cx - 1.5f * dp, cy + 5 * dp); visto.lineTo(cx + 6.5f * dp, cy - 4.5f * dp)
        vistoMedida.setPath(visto, false)
        vistoParcial.reset()
        vistoMedida.getSegment(0f, vistoMedida.length * (t / 0.22f).coerceIn(0f, 1f), vistoParcial, true)
        traco.color = 0xFF4ADE80.toInt(); traco.alpha = alfa; traco.strokeWidth = 2.5f * dp
        c.drawPath(vistoParcial, traco)
    }

    private companion object {
        const val ESCALA = 1.35f // 30 dp de altura ficou pequeno no POCO; 40,5 dp
    }
}
