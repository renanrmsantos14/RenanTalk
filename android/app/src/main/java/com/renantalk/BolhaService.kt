package com.renantalk

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.util.DisplayMetrics
import android.view.animation.PathInterpolator
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import android.widget.TextView
import android.widget.Toast
import java.util.concurrent.Executors
import kotlin.math.abs

/**
 * O RenanTalk flutua sobre o teclado que estiver aberto (o Gboard, por exemplo):
 * - parado: a pílula "Ditar" em cima do teclado (arrastável para os lados, a posição fica salva);
 * - gravando: a pílula vira a ilha (✕ · ondas · tempo · ↑) e uma luz contorna a tela, reagindo à voz;
 * - avisos: uma legenda acima da pílula; nos erros a luz da tela fica vermelha.
 * Segurar = fala e solta para enviar (arrastar para cima cancela); tocar = começa, tocar de novo = envia (✕ cancela).
 * O texto entra no campo focado pela acessibilidade (ACTION_SET_TEXT; se o app recusar, cola).
 */
class BolhaService : AccessibilityService() {
    private enum class Fase { OCIOSA, GRAVANDO, TRANSCREVENDO }

    private val principal = Handler(Looper.getMainLooper())
    private val fundo = Executors.newSingleThreadExecutor()
    private val dp get() = resources.displayMetrics.density
    private fun px(v: Int) = (v * dp).toInt()
    private lateinit var wm: WindowManager

    // aba: a pílula/ilha, o único ponto de toque
    private lateinit var aba: IlhaView
    private lateinit var pAba: WindowManager.LayoutParams
    private val abaL get() = aba.pilulaL
    private var recolhendo = false // a ilha está voltando a ser pílula: a janela ainda está aberta

    // luz em volta da tela e legenda dos avisos (não recebem toques)
    private lateinit var contorno: ContornoView
    private lateinit var pContorno: WindowManager.LayoutParams
    private lateinit var legenda: TextView
    private lateinit var pLegenda: WindowManager.LayoutParams

    private var abaVisivel = false
    private var contornoVisivel = false
    private var legendaVisivel = false
    private val teclado = Rect()
    private var fase = Fase.OCIOSA
    private var modoToque = false
    private var tocouEm = 0L
    private var xToque = 0f
    private var yToque = 0f
    private var xAbaNoToque = 0
    private var arrastando = false
    private var comecouNesteToque = false
    private var tocando = false // o DOWN foi aceito: só então MOVE/UP valem
    private var gravouEm = 0L
    private var alvo: AccessibilityNodeInfo? = null
    private val gravador = Gravador { n -> contorno.audio(n); aba.audio(n) }
    private val suave = PathInterpolator(0.23f, 1f, 0.32f, 1f)
    private val prefs by lazy { getSharedPreferences("bolha", MODE_PRIVATE) }

    private val iniciar = Runnable { comecar() }
    private val apagarAviso = Runnable { esconderLegenda() }

    override fun onServiceConnected() {
        wm = getSystemService(WINDOW_SERVICE) as WindowManager
        montar()
    }

    override fun onAccessibilityEvent(e: AccessibilityEvent?) {
        if (fase != Fase.OCIOSA || arrastando) return
        val janela = windows.firstOrNull { it.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD }
        if (janela != null) {
            janela.getBoundsInScreen(teclado)
            if (!recolhendo) mostrarAba()
        } else {
            esconderAba()
            esconderLegenda()
        }
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        principal.removeCallbacksAndMessages(null)
        fundo.shutdown()
        if (gravador.ativo) gravador.parar()
        esconderAba()
        esconderLegenda()
        esconderContorno()
        super.onDestroy()
    }

    // ---------- janelas ----------

    private fun janela(largura: Int, altura: Int, tocavel: Boolean) = WindowManager.LayoutParams(
        largura, altura,
        WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS or
            (if (tocavel) 0 else WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE),
        PixelFormat.TRANSLUCENT,
    ).apply { gravity = Gravity.TOP or Gravity.START }

    private fun montar() {
        aba = IlhaView(this).apply {
            setOnTouchListener { _, ev -> tocar(ev); true }
            aoRecolher = { recolhido() }
        }
        pAba = janela(aba.pilulaL, aba.janelaA, tocavel = true)

        contorno = ContornoView(this).apply { aoApagar = { esconderContorno() } }
        pContorno = janela(-1, -1, tocavel = false).apply {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R)
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }

        legenda = TextView(this).apply {
            textSize = 15f
            typeface = resources.getFont(R.font.geist_semibold)
            setTextColor(0xFFDBEAFE.toInt())
            gravity = Gravity.CENTER
            setPadding(px(16), px(9), px(16), px(9))
            background = GradientDrawable().apply {
                cornerRadius = 18 * dp
                setColor(0xF5080B12.toInt())
                setStroke(px(1), 0x24FFFFFF)
            }
        }
        pLegenda = janela(-2, -2, tocavel = false)
    }

    /** x da aba em px, a partir da fração salva (0 = canto esquerdo do teclado, 1 = direito). */
    private fun xAba(fracao: Float): Int {
        val min = teclado.left + px(ABA_LADO) - aba.margem
        val max = teclado.right - abaL - px(ABA_LADO) + aba.margem
        return (min + (max - min) * fracao).toInt().coerceIn(min, maxOf(min, max))
    }

    private fun mostrarAba() {
        pAba.width = abaL
        pAba.x = xAba(prefs.getFloat(POSICAO, 0.5f))
        pAba.y = teclado.top - aba.janelaA + aba.margem - px(ABA_FOLGA)
        aba.ancora = 0f
        if (!abaVisivel) {
            aba.alpha = 0f
            aba.translationY = px(6).toFloat()
            aba.scaleX = 0.92f; aba.scaleY = 0.92f
            wm.addView(aba, pAba)
            abaVisivel = true
            aba.repousar()
            aba.animate().alpha(1f).translationY(0f).scaleX(1f).scaleY(1f).setDuration(240).setInterpolator(suave).start()
        } else wm.updateViewLayout(aba, pAba)
        if (legendaVisivel) posicionarLegenda()
    }

    private fun esconderAba() {
        principal.removeCallbacks(iniciar) // teclado fechou nos 120 ms de espera: não começa a gravar sem a aba
        recolhendo = false
        if (!abaVisivel) return
        abaVisivel = false
        runCatching { wm.removeView(aba) }
        aba.definir(IlhaView.Modo.DITAR)
    }

    /** Abre a janela da aba para a ilha caber, mantendo a cápsula no lugar e centrada na pílula. */
    private fun abrirIlha() {
        if (!abaVisivel) return
        val centro = pAba.x + pAba.width / 2
        val antes = pAba.x
        pAba.width = aba.abertaL
        pAba.x = (centro - aba.abertaL / 2).coerceIn(teclado.left, maxOf(teclado.left, teclado.right - aba.abertaL))
        aba.deslocar((antes - pAba.x).toFloat())
        wm.updateViewLayout(aba, pAba)
    }

    /** A ilha volta a ser pílula na posição salva; a janela só encolhe quando a animação acaba. */
    private fun fecharIlha(modo: IlhaView.Modo) {
        if (abaVisivel && pAba.width != abaL) {
            recolhendo = true
            aba.ancora = (xAba(prefs.getFloat(POSICAO, 0.5f)) - pAba.x).toFloat()
        }
        aba.definir(modo)
    }

    private fun recolhido() {
        if (!recolhendo) return
        recolhendo = false
        val antes = pAba.x
        mostrarAba()
        aba.deslocar((antes - pAba.x).toFloat())
        onAccessibilityEvent(null) // o teclado pode ter fechado enquanto a ilha recolhia
    }

    private fun mostrarContorno() {
        val m = DisplayMetrics()
        @Suppress("DEPRECATION") wm.defaultDisplay.getRealMetrics(m)
        pContorno.width = m.widthPixels
        pContorno.height = m.heightPixels
        if (!contornoVisivel) {
            wm.addView(contorno, pContorno)
            contornoVisivel = true
        } else wm.updateViewLayout(contorno, pContorno)
    }

    private fun esconderContorno() {
        if (!contornoVisivel) return
        contornoVisivel = false
        runCatching { wm.removeView(contorno) }
    }

    /** A legenda fica centrada sobre o teclado, logo acima da pílula. */
    private fun posicionarLegenda() {
        legenda.measure(View.MeasureSpec.makeMeasureSpec(teclado.width() - px(32), View.MeasureSpec.AT_MOST),
            View.MeasureSpec.UNSPECIFIED)
        pLegenda.width = legenda.measuredWidth
        pLegenda.height = legenda.measuredHeight
        pLegenda.x = teclado.left + (teclado.width() - legenda.measuredWidth) / 2
        pLegenda.y = pAba.y + aba.margem - px(8) - legenda.measuredHeight
        if (legendaVisivel) wm.updateViewLayout(legenda, pLegenda)
    }

    private fun mostrarLegenda() {
        principal.removeCallbacks(apagarAviso)
        posicionarLegenda()
        if (!legendaVisivel) {
            legenda.alpha = 0f
            legenda.translationY = px(6).toFloat()
            wm.addView(legenda, pLegenda)
            legendaVisivel = true
        }
        legenda.animate().alpha(1f).translationY(0f).setDuration(200).setInterpolator(suave).start()
    }

    private fun esconderLegenda() {
        principal.removeCallbacks(apagarAviso)
        if (!legendaVisivel) return
        legendaVisivel = false
        runCatching { wm.removeView(legenda) }
    }

    // ---------- gestos ----------

    private fun tocar(ev: MotionEvent) {
        val folga = ViewConfiguration.get(this).scaledTouchSlop
        if (ev.actionMasked != MotionEvent.ACTION_DOWN && !tocando) return
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (recolhendo || fase == Fase.TRANSCREVENDO) return
                tocando = true
                aba.animate().scaleX(0.96f).scaleY(0.96f).setDuration(100).setInterpolator(suave).start()
                tocouEm = SystemClock.uptimeMillis()
                xToque = ev.rawX; yToque = ev.rawY
                xAbaNoToque = pAba.x
                arrastando = false
                comecouNesteToque = false
                // espera um instante para distinguir arrastar a aba de segurar para falar
                if (fase == Fase.OCIOSA) principal.postDelayed(iniciar, ESPERA_MS)
            }
            MotionEvent.ACTION_MOVE -> {
                val dx = ev.rawX - xToque
                val dy = yToque - ev.rawY
                when {
                    arrastando -> {
                        pAba.x = (xAbaNoToque + dx).toInt().coerceIn(xAba(0f), xAba(1f))
                        wm.updateViewLayout(aba, pAba)
                    }
                    fase == Fase.OCIOSA && abs(dx) > folga && abs(dx) > abs(dy) -> {
                        principal.removeCallbacks(iniciar)
                        arrastando = true
                    }
                    fase == Fase.GRAVANDO && !modoToque && dy > px(CANCELAR_DP) -> cancelar()
                }
            }
            MotionEvent.ACTION_UP -> { tocando = false; soltar(ev.x) }
            MotionEvent.ACTION_CANCEL -> {
                tocando = false
                aba.animate().scaleX(1f).scaleY(1f).setDuration(160).setInterpolator(suave).start()
                principal.removeCallbacks(iniciar)
                arrastando = false
                if (fase == Fase.GRAVANDO && !modoToque) parar()
            }
        }
    }

    private fun soltar(x: Float) {
        aba.animate().scaleX(1f).scaleY(1f).setDuration(160).setInterpolator(suave).start()
        when {
            arrastando -> {
                arrastando = false
                val min = xAba(0f); val max = xAba(1f)
                if (max > min) prefs.edit().putFloat(POSICAO, (pAba.x - min).toFloat() / (max - min)).apply()
            }
            fase == Fase.OCIOSA -> { // toque rápido: começa no modo toque
                principal.removeCallbacks(iniciar)
                comecar()
                if (fase == Fase.GRAVANDO) modoToque = true
            }
            fase != Fase.GRAVANDO -> Unit
            comecouNesteToque && SystemClock.uptimeMillis() - tocouEm < SEGURAR_MS -> modoToque = true
            modoToque && aba.tocouCancelar(x) -> cancelar()
            else -> parar()
        }
    }

    private fun comecar() {
        alvo = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        runCatching { gravador.comecar() }.onFailure {
            avisar("Sem acesso ao microfone. Abra o RenanTalk e permita o microfone.", erro = true)
            return
        }
        fase = Fase.GRAVANDO
        modoToque = false
        comecouNesteToque = true
        gravouEm = SystemClock.uptimeMillis()
        esconderLegenda()
        aba.inicio = gravouEm
        abrirIlha()
        aba.definir(IlhaView.Modo.GRAVANDO)
        mostrarContorno()
        contorno.definir(ContornoView.Estado.REC)
        vibrar(forte = true)
    }

    private fun cancelar() {
        gravador.parar()
        vibrar()
        voltar()
        contorno.definir(ContornoView.Estado.PARADA)
        avisar("Cancelado.")
    }

    private fun parar() {
        val g = gravador.parar()
        when (Regras.decidir(g.pcm, g.duracaoS)) {
            Regras.Decisao.CURTO -> { voltar(); contorno.definir(ContornoView.Estado.PARADA); avisar("Segure a aba enquanto fala, ou toque para começar.") }
            Regras.Decisao.SILENCIO -> { voltar(); avisar("Não ouvi voz. Fale mais perto.", erro = true) }
            Regras.Decisao.ENVIAR -> enviar(g.pcm)
        }
    }

    private fun enviar(pcm: ByteArray) {
        fase = Fase.TRANSCREVENDO
        contorno.definir(ContornoView.Estado.PROC)
        aba.definir(IlhaView.Modo.ESCREVENDO)
        vibrar()
        val chave = Cofre(this).ler()
        val prompt = Vocabulario(this).prompt()
        fundo.execute {
            val resultado = runCatching { Groq.transcrever(chave, Regras.wav(pcm), prompt) }
            principal.post {
                resultado.onSuccess { texto ->
                    when {
                        Regras.semFala(texto) -> { voltar(); avisar("Não entendi. Tente de novo.", erro = true) }
                        !inserir(texto) -> { voltar(); contorno.definir(ContornoView.Estado.FIM); avisar("Texto copiado: cole no campo.") }
                        else -> { voltar(IlhaView.Modo.OK); contorno.definir(ContornoView.Estado.FIM) }
                    }
                }.onFailure { erro ->
                    voltar()
                    avisar(if (erro is Groq.Erro) erro.message!! else "Sem conexão com a Groq.", erro = true)
                }
            }
        }
    }

    private fun voltar(modo: IlhaView.Modo = IlhaView.Modo.DITAR) {
        fase = Fase.OCIOSA
        modoToque = false
        fecharIlha(modo)
        if (!recolhendo) onAccessibilityEvent(null) // o teclado pode ter fechado enquanto transcrevia
    }

    // ---------- avisos ----------

    /** Aviso numa legenda acima da pílula (erro também acende a tela em vermelho); sem teclado, toast. */
    private fun avisar(texto: String, erro: Boolean = false) {
        if (erro) { mostrarContorno(); contorno.definir(ContornoView.Estado.ERRO); vibrar() }
        if (!abaVisivel) {
            esconderLegenda()
            Toast.makeText(this, texto, Toast.LENGTH_SHORT).show()
            return
        }
        legenda.text = texto
        legenda.setTextColor(if (erro) 0xFFFECACA.toInt() else 0xFFDBEAFE.toInt())
        mostrarLegenda()
        principal.postDelayed(apagarAviso, AVISO_MS)
    }

    // ---------- inserir ----------

    /** Insere no cursor do campo que estava focado; se o app não aceitar, copia e tenta colar. */
    private fun inserir(texto: String): Boolean {
        val campo = alvo?.takeIf { it.refresh() } ?: rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?: return copiar(texto)
        if (campo.isEditable) {
            val atual = Regras.textoDoCampo(campo.text?.toString(), campo.hintText?.toString(), campo.isShowingHintText)
            val (novo, cursor) = Regras.inserir(atual, campo.textSelectionStart, campo.textSelectionEnd, texto)
            val ok = campo.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, Bundle().apply {
                putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, novo)
            })
            if (ok) {
                campo.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, Bundle().apply {
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, cursor)
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, cursor)
                })
                return true
            }
        }
        copiar(texto)
        return campo.performAction(AccessibilityNodeInfo.ACTION_PASTE)
    }

    private fun copiar(texto: String): Boolean {
        (getSystemService(CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("RenanTalk", texto))
        return false
    }

    private fun vibrar(forte: Boolean = false) {
        val v = getSystemService(Vibrator::class.java) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q)
            v.vibrate(VibrationEffect.createPredefined(if (forte) VibrationEffect.EFFECT_HEAVY_CLICK else VibrationEffect.EFFECT_CLICK))
        else v.vibrate(VibrationEffect.createOneShot(if (forte) 30 else 15, VibrationEffect.DEFAULT_AMPLITUDE))
    }

    private companion object {
        const val SEGURAR_MS = 350L
        const val ESPERA_MS = 120L // tempo para decidir entre arrastar a aba e segurar para falar
        const val AVISO_MS = 2500L
        const val CANCELAR_DP = 72
        const val ABA_FOLGA = 6 // a pílula flutua acima do teclado
        const val ABA_LADO = 12 // distância mínima da pílula até a lateral da tela
        const val POSICAO = "posicao" // fração 0..1 da pílula sobre o teclado (0,5 = centro)
    }
}
