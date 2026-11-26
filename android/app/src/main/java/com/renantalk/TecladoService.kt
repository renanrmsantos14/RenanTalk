package com.renantalk

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import java.util.concurrent.Executors

/** Teclado RenanTalk: segure o microfone, fale e solte; o texto entra no campo focado. */
class TecladoService : InputMethodService() {
    private val principal = Handler(Looper.getMainLooper())
    private val fundo = Executors.newSingleThreadExecutor()
    private lateinit var luz: LuzView
    private lateinit var mic: ImageView
    private lateinit var status: TextView
    private val gravador = Gravador { nivel -> if (::luz.isInitialized) luz.audio(nivel) }
    private var campoSenha = false
    private var transcrevendo = false
    private val restaurarStatus = Runnable { mostrarVersao() }

    private val dp get() = resources.displayMetrics.density
    private fun cor(id: Int) = getColor(id)

    @SuppressLint("ClickableViewAccessibility")
    override fun onCreateInputView(): View {
        val raiz = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(cor(R.color.superficie))
            setPadding((6 * dp).toInt(), 0, (6 * dp).toInt(), (4 * dp).toInt())
        }
        luz = LuzView(this)
        raiz.addView(luz, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (10 * dp).toInt()))

        val linha = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        val globo = tecla(R.drawable.ic_globo, "Trocar teclado")
        globo.setOnClickListener { toque(it); trocarTeclado() }
        globo.setOnLongClickListener {
            (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showInputMethodPicker(); true
        }
        val apagar = tecla(R.drawable.ic_apagar, "Apagar")
        repetirAoSegurar(apagar) { sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL) }

        mic = ImageView(this).apply {
            setImageResource(R.drawable.ic_mic)
            imageTintList = ColorStateList.valueOf(0xFFFFFFFF.toInt())
            scaleType = ImageView.ScaleType.CENTER
            contentDescription = "Segure para falar"
            background = pilula(cor(R.color.azul))
            setOnTouchListener { v, e -> aoTocarMic(v, e) }
        }
        val espaco = tecla(R.drawable.ic_espaco, "Espaço")
        espaco.setOnClickListener { toque(it); currentInputConnection?.commitText(" ", 1) }
        val enter = tecla(R.drawable.ic_enter, "Enter")
        enter.setOnClickListener { toque(it); enter() }

        val alt = (56 * dp).toInt()
        val margem = (3 * dp).toInt()
        fun lp(larg: Int, peso: Float = 0f) = LinearLayout.LayoutParams(larg, alt, peso).apply { setMargins(margem, margem, margem, margem) }
        linha.addView(globo, lp((52 * dp).toInt()))
        linha.addView(apagar, lp((52 * dp).toInt()))
        linha.addView(mic, lp(0, 1f))
        linha.addView(espaco, lp((60 * dp).toInt()))
        linha.addView(enter, lp((52 * dp).toInt()))
        raiz.addView(linha)

        status = TextView(this).apply {
            textSize = 11f
            setTextColor(cor(R.color.texto_fraco))
            gravity = Gravity.END
            setPadding(0, 0, (6 * dp).toInt(), 0)
            setOnClickListener { abrirConfiguracoes() }
        }
        raiz.addView(status, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        mostrarVersao()
        procurarAtualizacaoSeVencido()
        return raiz
    }

    override fun onStartInputView(info: EditorInfo, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        val classe = info.inputType and InputType.TYPE_MASK_CLASS
        val variacao = info.inputType and InputType.TYPE_MASK_VARIATION
        campoSenha = (classe == InputType.TYPE_CLASS_TEXT && variacao in setOf(
            InputType.TYPE_TEXT_VARIATION_PASSWORD, InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD,
            InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,
        )) || (classe == InputType.TYPE_CLASS_NUMBER && variacao == InputType.TYPE_NUMBER_VARIATION_PASSWORD)
        if (::mic.isInitialized) mic.alpha = if (campoSenha) 0.35f else 1f
        if (::status.isInitialized) mostrarVersao()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        if (gravador.ativo) { // teclado fechou no meio: descarta
            gravador.parar()
            luz.definir(LuzView.Estado.FEITO)
        }
        super.onFinishInputView(finishingInput)
    }

    override fun onDestroy() {
        fundo.shutdownNow()
        super.onDestroy()
    }

    // ---------- ditado ----------

    private fun aoTocarMic(v: View, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> comecar(v)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> parar(v)
        }
        return true
    }

    private fun comecar(v: View) {
        if (campoSenha) return avisar("Ditado desligado em campo de senha.")
        if (transcrevendo || gravador.ativo) return
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            return abrirConfiguracoes("Permita o microfone nas configurações.")
        }
        if (Cofre(this).ler().isEmpty()) return abrirConfiguracoes("Cadastre a chave Groq nas configurações.")
        try {
            gravador.comecar()
        } catch (e: Exception) {
            luz.definir(LuzView.Estado.ERRO)
            return avisar(e.message ?: "Microfone indisponível.")
        }
        v.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        v.isPressed = true
        mic.background = pilula(cor(R.color.azul_escuro))
        luz.definir(LuzView.Estado.REC)
        mensagem("Ouvindo… solte para enviar", fixa = true)
    }

    private fun parar(v: View) {
        if (!gravador.ativo) return
        val g = gravador.parar()
        v.isPressed = false
        mic.background = pilula(cor(R.color.azul))
        when (Regras.decidir(g.pcm, g.duracaoS)) {
            Regras.Decisao.CURTO -> { luz.definir(LuzView.Estado.FEITO); mensagem("Segure o microfone enquanto fala.") }
            Regras.Decisao.SILENCIO -> { luz.definir(LuzView.Estado.FEITO); mensagem("Não ouvi voz.") }
            Regras.Decisao.ENVIAR -> enviar(v, g.pcm)
        }
    }

    private fun enviar(v: View, pcm: ByteArray) {
        transcrevendo = true
        v.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        luz.definir(LuzView.Estado.PROC)
        mensagem("Transcrevendo…", fixa = true)
        val chave = Cofre(this).ler() // relida a cada ditado: troca nas configurações vale na hora
        val prompt = Vocabulario(this).prompt()
        fundo.execute {
            val resultado = runCatching { Groq.transcrever(chave, Regras.wav(pcm), prompt) }
            principal.post {
                transcrevendo = false
                resultado.onSuccess { texto ->
                    if (texto.isNotEmpty()) currentInputConnection?.commitText("$texto ", 1)
                    luz.definir(LuzView.Estado.FEITO)
                    mostrarVersao()
                }.onFailure { erro ->
                    luz.definir(LuzView.Estado.ERRO)
                    v.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                    mensagem(if (erro is Groq.Erro) erro.message!! else "Sem conexão com a Groq.")
                }
            }
        }
    }

    // ---------- teclas ----------

    private fun trocarTeclado() {
        val trocou = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && switchToNextInputMethod(false)
        if (!trocou) (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showInputMethodPicker()
    }

    private fun enter() {
        val info = currentInputEditorInfo
        val acao = (info?.imeOptions ?: 0) and EditorInfo.IME_MASK_ACTION
        val semAcao = (info?.imeOptions ?: 0) and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0
        if (!semAcao && acao != EditorInfo.IME_ACTION_NONE && acao != EditorInfo.IME_ACTION_UNSPECIFIED) {
            currentInputConnection?.performEditorAction(acao)
        } else {
            sendDownUpKeyEvents(KeyEvent.KEYCODE_ENTER)
        }
    }

    @SuppressLint("ClickableViewAccessibility")
    private fun repetirAoSegurar(v: View, acao: () -> Unit) {
        val repetir = object : Runnable {
            override fun run() { acao(); principal.postDelayed(this, 60) }
        }
        v.setOnTouchListener { view, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { view.isPressed = true; toque(view); acao(); principal.postDelayed(repetir, 400) }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> { view.isPressed = false; principal.removeCallbacks(repetir) }
            }
            true
        }
    }

    private fun tecla(icone: Int, descricao: String) = ImageView(this).apply {
        setImageResource(icone)
        imageTintList = ColorStateList.valueOf(cor(R.color.texto))
        scaleType = ImageView.ScaleType.CENTER
        contentDescription = descricao
        background = pilula(cor(R.color.tecla))
        isClickable = true
    }

    private fun pilula(c: Int) = GradientDrawable().apply { cornerRadius = 14 * dp; setColor(c) }

    private fun toque(v: View) = v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)

    // ---------- status, configurações e atualização ----------

    private fun mostrarVersao() {
        val nova = getSharedPreferences("atualizacao", MODE_PRIVATE).getString("nova", null)
        status.setTypeface(null, Typeface.NORMAL)
        status.text = if (nova != null && Regras.versaoMaior(nova, BuildConfig.VERSION_NAME)) {
            status.setTypeface(null, Typeface.BOLD)
            "Nova versão $nova · toque para atualizar"
        } else {
            "v${BuildConfig.VERSION_NAME} · ${BuildConfig.VERSAO_DATA}"
        }
    }

    private fun mensagem(texto: String, fixa: Boolean = false) {
        principal.removeCallbacks(restaurarStatus)
        status.setTypeface(null, Typeface.NORMAL)
        status.text = texto
        if (!fixa) principal.postDelayed(restaurarStatus, 3500)
    }

    private fun avisar(texto: String) = mensagem(texto)

    private fun abrirConfiguracoes(motivo: String? = null) {
        motivo?.let { mensagem(it) }
        startActivity(Intent(this, ConfigActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    private fun procurarAtualizacaoSeVencido() {
        if (Atualizador.ehDev) return
        val prefs = getSharedPreferences("atualizacao", MODE_PRIVATE)
        val agora = System.currentTimeMillis()
        if (agora - prefs.getLong("ultima", 0) < Atualizador.INTERVALO_MS) return
        prefs.edit().putLong("ultima", agora).apply()
        fundo.execute {
            runCatching { Atualizador.procurar() }.onSuccess { nova ->
                prefs.edit().putString("nova", nova?.versao).apply()
                principal.post { mostrarVersao() }
            }
        }
    }
}
