package com.renantalk

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
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
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.Toast
import java.util.concurrent.Executors

/**
 * Bolha flutuante (como o Wispr Flow): aparece acima do teclado que estiver aberto e some quando ele fecha.
 * Segurar = fala e solta para enviar; tocar = começa, tocar de novo = envia.
 * O texto entra no campo focado pela acessibilidade (ACTION_SET_TEXT; se o app recusar, cola).
 */
class BolhaService : AccessibilityService() {
    private enum class Fase { OCIOSA, GRAVANDO, TRANSCREVENDO }

    private val principal = Handler(Looper.getMainLooper())
    private val fundo = Executors.newSingleThreadExecutor()
    private val dp get() = resources.displayMetrics.density
    private lateinit var wm: WindowManager
    private lateinit var raiz: FrameLayout
    private lateinit var mic: ImageView
    private lateinit var onda: OndaView
    private lateinit var params: WindowManager.LayoutParams
    private var visivel = false
    private var fase = Fase.OCIOSA
    private var modoToque = false
    private var tocouEm = 0L
    private var comecouNesteToque = false
    private var alvo: AccessibilityNodeInfo? = null
    private val gravador = Gravador { n -> onda.audio(n) }
    private val destaque = 0xFF3B82F6.toInt()

    override fun onServiceConnected() {
        wm = getSystemService(WINDOW_SERVICE) as WindowManager
        montar()
    }

    override fun onAccessibilityEvent(e: AccessibilityEvent?) {
        if (fase != Fase.OCIOSA) return
        val teclado = Rect()
        val aberto = windows.firstOrNull { it.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD }
            ?.also { it.getBoundsInScreen(teclado) } != null
        if (aberto) mostrar(teclado) else esconder()
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        if (gravador.ativo) gravador.parar()
        esconder()
        super.onDestroy()
    }

    // ---------- bolha ----------

    private fun montar() {
        mic = ImageView(this).apply {
            setImageResource(R.drawable.ic_mic)
            setColorFilter(0xFFFFFFFF.toInt())
            scaleType = ImageView.ScaleType.CENTER
            contentDescription = "Ditar com o RenanTalk"
        }
        onda = OndaView(this).apply { cor = destaque; visibility = View.GONE }
        raiz = FrameLayout(this).apply {
            addView(onda, FrameLayout.LayoutParams(-1, -1).apply { setMargins((14 * dp).toInt(), 0, (14 * dp).toInt(), 0) })
            addView(mic, FrameLayout.LayoutParams(-1, -1))
            elevation = 6 * dp
            setOnTouchListener { _, ev -> tocar(ev); true }
        }
        pintar()
        params = WindowManager.LayoutParams(
            tamanho(), tamanho(),
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.TOP or Gravity.END }
    }

    private fun tamanho() = (48 * dp).toInt()

    private fun mostrar(teclado: Rect) {
        params.x = (12 * dp).toInt()
        params.y = teclado.top - tamanho() - (8 * dp).toInt()
        if (!visivel) {
            raiz.alpha = 0f; raiz.scaleX = 0.6f; raiz.scaleY = 0.6f
            wm.addView(raiz, params)
            visivel = true
            raiz.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(160).start()
        } else wm.updateViewLayout(raiz, params)
    }

    private fun esconder() {
        if (!visivel) return
        visivel = false
        runCatching { wm.removeView(raiz) }
    }

    /** Ociosa = círculo azul com microfone; gravando/transcrevendo = pílula escura com a linha de luz. */
    private fun pintar() {
        val ociosa = fase == Fase.OCIOSA
        raiz.background = GradientDrawable().apply {
            cornerRadius = 24 * dp
            setColor(if (ociosa) destaque else 0xF0131C2E.toInt())
        }
        mic.visibility = if (ociosa) View.VISIBLE else View.GONE
        onda.visibility = if (ociosa) View.GONE else View.VISIBLE
        if (::params.isInitialized && visivel) {
            params.width = if (ociosa) tamanho() else (170 * dp).toInt()
            wm.updateViewLayout(raiz, params)
        }
    }

    // ---------- gestos ----------

    private fun tocar(ev: MotionEvent) {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                tocouEm = SystemClock.uptimeMillis()
                comecouNesteToque = false
                if (fase == Fase.OCIOSA) comecar()
            }
            MotionEvent.ACTION_UP -> when {
                fase != Fase.GRAVANDO -> Unit
                comecouNesteToque && SystemClock.uptimeMillis() - tocouEm < SEGURAR_MS -> modoToque = true // toque: segue gravando
                else -> parar()
            }
            MotionEvent.ACTION_CANCEL -> if (fase == Fase.GRAVANDO && !modoToque) parar()
        }
    }

    private fun comecar() {
        alvo = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        runCatching { gravador.comecar() }.onFailure {
            avisar("Sem acesso ao microfone. Abra o RenanTalk e permita o microfone.")
            return
        }
        fase = Fase.GRAVANDO
        modoToque = false
        comecouNesteToque = true
        onda.definir(OndaView.Estado.REC)
        vibrar(forte = true)
        pintar()
    }

    private fun parar() {
        val g = gravador.parar()
        when (Regras.decidir(g.pcm, g.duracaoS)) {
            Regras.Decisao.CURTO -> { voltar(); avisar("Segure a bolha enquanto fala, ou toque para começar.") }
            Regras.Decisao.SILENCIO -> { voltar(); avisar("Não ouvi voz.") }
            Regras.Decisao.ENVIAR -> enviar(g.pcm)
        }
    }

    private fun enviar(pcm: ByteArray) {
        fase = Fase.TRANSCREVENDO
        onda.definir(OndaView.Estado.PROC)
        vibrar()
        val chave = Cofre(this).ler()
        val prompt = Vocabulario(this).prompt()
        fundo.execute {
            val resultado = runCatching { Groq.transcrever(chave, Regras.wav(pcm), prompt) }
            principal.post {
                resultado.onSuccess { texto -> if (texto.isNotEmpty() && !inserir(texto)) avisar("Texto copiado: cole no campo.") }
                    .onFailure { erro -> avisar(if (erro is Groq.Erro) erro.message!! else "Sem conexão com a Groq.") }
                voltar()
            }
        }
    }

    private fun voltar() {
        fase = Fase.OCIOSA
        onda.definir(OndaView.Estado.PARADA)
        pintar()
        onAccessibilityEvent(null) // o teclado pode ter fechado enquanto transcrevia
    }

    // ---------- inserir ----------

    /** Insere no cursor do campo que estava focado; se o app não aceitar, copia e tenta colar. */
    private fun inserir(texto: String): Boolean {
        val campo = alvo?.takeIf { it.refresh() } ?: rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?: return copiar(texto)
        if (campo.isEditable) {
            val atual = if (campo.isShowingHintText) "" else campo.text?.toString().orEmpty()
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

    private fun avisar(texto: String) = Toast.makeText(this, texto, Toast.LENGTH_SHORT).show()

    private fun vibrar(forte: Boolean = false) {
        val v = getSystemService(Vibrator::class.java) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q)
            v.vibrate(VibrationEffect.createPredefined(if (forte) VibrationEffect.EFFECT_HEAVY_CLICK else VibrationEffect.EFFECT_CLICK))
        else v.vibrate(VibrationEffect.createOneShot(if (forte) 30 else 15, VibrationEffect.DEFAULT_AMPLITUDE))
    }

    private companion object { const val SEGURAR_MS = 350L }
}
