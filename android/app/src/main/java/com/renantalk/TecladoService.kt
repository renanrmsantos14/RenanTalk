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
import android.widget.FrameLayout
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

    // Layout copiado do Gboard do Renan: fileira de números, QWERTY, ⇧/⌫, ?123 e espaço.
    private enum class Shift { NAO, UMA, TRAVADA }
    private var shift = Shift.NAO
    private var ultimoShift = 0L
    private var simbolos = false
    private lateinit var teclas: LinearLayout
    private lateinit var variantes: LinearLayout
    private val letras = mutableListOf<TextView>()
    private var shiftTecla: TextView? = null

    override fun onCreateInputView(): View {
        val raiz = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(cor(R.color.superficie))
            setPadding((3 * dp).toInt(), 0, (3 * dp).toInt(), (4 * dp).toInt())
        }
        luz = LuzView(this)
        raiz.addView(luz, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (10 * dp).toInt()))

        // Barra no lugar das sugestões do Gboard: versão/mensagens ou os acentos da tecla segurada.
        val barra = FrameLayout(this)
        status = TextView(this).apply {
            textSize = 11f
            setTextColor(cor(R.color.texto_fraco))
            gravity = Gravity.END or Gravity.CENTER_VERTICAL
            setPadding(0, 0, (8 * dp).toInt(), 0)
            setOnClickListener { abrirConfiguracoes() }
        }
        variantes = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            visibility = View.GONE
        }
        val cheio = FrameLayout.LayoutParams.MATCH_PARENT
        barra.addView(status, FrameLayout.LayoutParams(cheio, cheio))
        barra.addView(variantes, FrameLayout.LayoutParams(cheio, cheio))
        raiz.addView(barra, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (36 * dp).toInt()))

        teclas = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        raiz.addView(teclas)
        montar()
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
        simbolos = classe == InputType.TYPE_CLASS_NUMBER || classe == InputType.TYPE_CLASS_PHONE ||
            classe == InputType.TYPE_CLASS_DATETIME
        shift = Shift.NAO
        if (::teclas.isInitialized) { montar(); autoMaiuscula() }
        if (::status.isInitialized) mostrarVersao()
    }

    override fun onUpdateSelection(a: Int, b: Int, novoInicio: Int, novoFim: Int, c: Int, d: Int) {
        super.onUpdateSelection(a, b, novoInicio, novoFim, c, d)
        autoMaiuscula()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        esconderVariantes()
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

    @SuppressLint("ClickableViewAccessibility")
    private fun montar() {
        teclas.removeAllViews()
        letras.clear()
        shiftTecla = null
        esconderVariantes()
        val linhas = if (simbolos) 4 else 5
        val alt = (48 * dp * 5 / linhas).toInt() // altura total igual nas duas camadas
        val m = (2.5f * dp).toInt()
        fun linha(itens: List<Pair<View, Float>>, recuo: Float = 0f) {
            val l = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; weightSum = 10f }
            if (recuo > 0) l.addView(View(this), LinearLayout.LayoutParams(0, alt, recuo))
            itens.forEach { (v, peso) ->
                l.addView(v, LinearLayout.LayoutParams(0, alt, peso).apply { setMargins(m, m, m, m) })
            }
            if (recuo > 0) l.addView(View(this), LinearLayout.LayoutParams(0, alt, recuo))
            teclas.addView(l)
        }
        fun texto(fileira: String, registrar: Boolean = false) =
            fileira.map { simbolo(it.toString(), registrar) to 1f }

        linha(texto(Teclas.NUMEROS))
        val apagar = icone(R.drawable.ic_apagar, "Apagar").also { a ->
            repetirAoSegurar(a) { esconderVariantes(); sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL) }
        }
        if (simbolos) {
            linha(texto(Teclas.SIMBOLOS[0]))
            linha(texto(Teclas.SIMBOLOS[1]) + (apagar to 2f))
        } else {
            linha(texto(Teclas.LETRAS[0], true))
            linha(texto(Teclas.LETRAS[1], true), recuo = 0.5f)
            val sh = especial("⇧", "Maiúscula") { alternarShift() }
            shiftTecla = sh
            linha(listOf<Pair<View, Float>>(sh to 1.5f) + texto(Teclas.LETRAS[2], true) + (apagar to 1.5f))
            atualizarShift()
        }

        val camada = especial(if (simbolos) "ABC" else "?123", "Trocar letras e símbolos") {
            simbolos = !simbolos; montar(); autoMaiuscula()
        }.apply { textSize = 15f }
        val globo = icone(R.drawable.ic_globo, "Trocar teclado").apply {
            setOnClickListener { toque(it); trocarTeclado() }
            setOnLongClickListener { seletorDeTeclado(); true }
        }
        mic = ImageView(this).apply {
            setImageResource(R.drawable.ic_mic)
            imageTintList = ColorStateList.valueOf(0xFFFFFFFF.toInt())
            scaleType = ImageView.ScaleType.CENTER
            contentDescription = "Segure para falar"
            background = pilula(cor(R.color.azul))
            alpha = if (campoSenha) 0.35f else 1f
            setOnTouchListener { v, e -> aoTocarMic(v, e) }
        }
        val espaco = TextView(this).apply {
            text = "Português"
            textSize = 14f
            gravity = Gravity.CENTER
            setTextColor(cor(R.color.texto_fraco))
            background = pilula(cor(R.color.tecla))
            contentDescription = "Espaço"
            isClickable = true
            setOnClickListener { toque(it); digitar(" ") }
            setOnLongClickListener { seletorDeTeclado(); true }
        }
        val enter = icone(R.drawable.ic_enter, "Enter").apply { setOnClickListener { toque(it); enter() } }
        linha(listOf(
            camada to 1.2f, simbolo(",") to 0.9f, globo to 0.9f, mic to 1.3f,
            espaco to 3.4f, simbolo(".") to 0.9f, enter to 1.4f,
        ))
    }

    /** Tecla de texto; segurar mostra os acentos na barra (como o Gboard). */
    private fun simbolo(s: String, registrar: Boolean = false) = TextView(this).apply {
        text = s
        textSize = 22f
        gravity = Gravity.CENTER
        setTextColor(cor(R.color.texto))
        background = pilula(cor(R.color.tecla))
        isClickable = true
        setOnClickListener { toque(it); digitar(text.toString()) }
        val vars = Teclas.variantes(s[0])
        if (vars.isNotEmpty()) setOnLongClickListener { toque(it); mostrarVariantes(vars); true }
        if (registrar) letras += this
    }

    private fun especial(rotulo: String, descricao: String, acao: () -> Unit) = TextView(this).apply {
        text = rotulo
        textSize = 20f
        gravity = Gravity.CENTER
        setTextColor(cor(R.color.texto))
        background = pilula(cor(R.color.tecla_especial))
        contentDescription = descricao
        isClickable = true
        setOnClickListener { toque(it); acao() }
    }

    private fun icone(id: Int, descricao: String) = ImageView(this).apply {
        setImageResource(id)
        imageTintList = ColorStateList.valueOf(cor(R.color.texto))
        scaleType = ImageView.ScaleType.CENTER
        contentDescription = descricao
        background = pilula(cor(R.color.tecla_especial))
        isClickable = true
    }

    private fun digitar(s: String) {
        esconderVariantes()
        currentInputConnection?.commitText(s, 1)
        if (shift == Shift.UMA) { shift = Shift.NAO; atualizarShift() }
    }

    private fun alternarShift() {
        val agora = System.currentTimeMillis()
        shift = when {
            shift == Shift.TRAVADA -> Shift.NAO
            agora - ultimoShift < 350 -> Shift.TRAVADA // toque duplo trava
            shift == Shift.UMA -> Shift.NAO
            else -> Shift.UMA
        }
        ultimoShift = agora
        atualizarShift()
    }

    private fun autoMaiuscula() {
        if (simbolos || shift == Shift.TRAVADA || !::teclas.isInitialized) return
        val tipo = currentInputEditorInfo?.inputType ?: 0
        val caps = tipo != 0 && (currentInputConnection?.getCursorCapsMode(tipo) ?: 0) != 0
        val novo = if (caps) Shift.UMA else Shift.NAO
        if (novo != shift) { shift = novo; atualizarShift() }
    }

    private fun atualizarShift() {
        val maiuscula = shift != Shift.NAO
        letras.forEach { it.text = Teclas.caixa(it.text.toString(), maiuscula) }
        shiftTecla?.apply {
            text = if (shift == Shift.TRAVADA) "⇪" else "⇧"
            background = pilula(cor(if (maiuscula) R.color.azul else R.color.tecla_especial))
            setTextColor(if (maiuscula) 0xFFFFFFFF.toInt() else cor(R.color.texto))
        }
    }

    private fun mostrarVariantes(vars: List<String>) {
        variantes.removeAllViews()
        val m = (3 * dp).toInt()
        vars.forEach { v ->
            val s = Teclas.caixa(v, shift != Shift.NAO)
            variantes.addView(TextView(this).apply {
                text = s
                textSize = 20f
                gravity = Gravity.CENTER
                setTextColor(cor(R.color.texto))
                background = pilula(cor(R.color.tecla))
                isClickable = true
                setOnClickListener { toque(it); digitar(s) }
            }, LinearLayout.LayoutParams((46 * dp).toInt(), LinearLayout.LayoutParams.MATCH_PARENT).apply { setMargins(m, m, m, m) })
        }
        status.visibility = View.INVISIBLE
        variantes.visibility = View.VISIBLE
    }

    private fun esconderVariantes() {
        if (!::variantes.isInitialized || variantes.visibility != View.VISIBLE) return
        variantes.visibility = View.GONE
        status.visibility = View.VISIBLE
    }

    private fun seletorDeTeclado() =
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showInputMethodPicker()

    private fun trocarTeclado() {
        val trocou = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && switchToNextInputMethod(false)
        if (!trocou) seletorDeTeclado()
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

    private fun pilula(c: Int) = GradientDrawable().apply { cornerRadius = 8 * dp; setColor(c) }

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
