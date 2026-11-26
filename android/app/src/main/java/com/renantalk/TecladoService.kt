package com.renantalk

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.inputmethodservice.InputMethodService
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.text.InputType
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.view.Gravity
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.animation.DecelerateInterpolator
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.PopupWindow
import android.widget.TextView
import java.util.concurrent.Executors

/** Teclado RenanTalk: segure o microfone (ou o espaço), fale e solte; o texto entra no campo focado. */
class TecladoService : InputMethodService(), SharedPreferences.OnSharedPreferenceChangeListener {
    private val principal = Handler(Looper.getMainLooper())
    private val fundo = Executors.newSingleThreadExecutor()
    private val gravador = Gravador { nivel -> if (::onda.isInitialized) onda.audio(nivel) }
    private var campoSenha = false
    private var transcrevendo = false
    private var cancelando = false
    private var inicioGravacao = 0L
    private var ultimaVoz = 0L

    private var aj = Ajustes.Teclado()
    private lateinit var pal: Ajustes.Paleta

    private val dp get() = resources.displayMetrics.density

    // Layout copiado do Gboard do Renan: fileira de números, QWERTY, ⇧/⌫, ?123 e espaço.
    private enum class Shift { NAO, UMA, TRAVADA }
    private var shift = Shift.NAO
    private var ultimoShift = 0L
    private var simbolos = false
    private lateinit var raiz: LinearLayout
    private lateinit var superiores: LinearLayout
    private lateinit var inferior: LinearLayout
    private lateinit var mic: ImageView
    private var espacoTecla: TextView? = null
    private val letras = mutableListOf<TextView>()
    private var shiftTecla: ImageView? = null

    // Painel de voz: cobre as fileiras de cima enquanto grava, transcreve ou avisa.
    private lateinit var painel: LinearLayout
    private lateinit var painelTitulo: TextView
    private lateinit var painelDica: TextView
    private lateinit var onda: OndaView
    private val esconderPainelDepois = Runnable { esconderPainel() }
    private val relogio = object : Runnable {
        override fun run() {
            if (!gravador.ativo) return
            if (!cancelando) painelTitulo.text = "Ouvindo · ${Regras.cronometro(SystemClock.elapsedRealtime() - inicioGravacao)}"
            val agora = SystemClock.elapsedRealtime()
            if (onda.nivel > 0.3f) ultimaVoz = agora
            if (!cancelando) painelDica.text = if (agora - ultimaVoz > 1800) "Não estou ouvindo · fale mais perto do celular"
                else "Solte para enviar  ·  arraste para cima para cancelar"
            val escala = 1f + 0.22f * onda.nivel // o microfone pulsa com a voz
            mic.scaleX = escala; mic.scaleY = escala
            principal.postDelayed(this, 50)
        }
    }

    override fun onCreate() {
        super.onCreate()
        aj = Ajustes.ler(this)
        Ajustes.prefs(this).registerOnSharedPreferenceChangeListener(this)
    }

    /** Mudança feita na tela "Personalizar teclado" vale na hora, com o teclado aberto. */
    override fun onSharedPreferenceChanged(p: SharedPreferences?, chave: String?) {
        aj = Ajustes.ler(this)
        if (::raiz.isInitialized) { aplicarCores(); montar(); autoMaiuscula() }
    }

    override fun onCreateInputView(): View {
        raiz = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding((3 * dp).toInt(), (4 * dp).toInt(), (3 * dp).toInt(), (4 * dp).toInt())
        }
        val area = FrameLayout(this)
        superiores = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        area.addView(superiores, FrameLayout.LayoutParams(-1, -2))
        area.addView(montarPainel(), FrameLayout.LayoutParams(-1, -1))
        raiz.addView(area)
        inferior = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; weightSum = 10f }
        raiz.addView(inferior)
        aplicarCores()
        montar()
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
        if (::raiz.isInitialized) { aplicarCores(); montar(); autoMaiuscula() }
    }

    override fun onUpdateSelection(a: Int, b: Int, novoInicio: Int, novoFim: Int, c: Int, d: Int) {
        super.onUpdateSelection(a, b, novoInicio, novoFim, c, d)
        autoMaiuscula()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        esconderVariantes()
        esconderBalao()
        principal.removeCallbacks(segurarEspaco)
        if (gravador.ativo) { // teclado fechou no meio: descarta
            gravador.parar()
            principal.removeCallbacks(relogio)
            mic.scaleX = 1f; mic.scaleY = 1f
        }
        if (::painel.isInitialized && !transcrevendo) esconderPainel(imediato = true)
        super.onFinishInputView(finishingInput)
    }

    override fun onDestroy() {
        Ajustes.prefs(this).unregisterOnSharedPreferenceChangeListener(this)
        fundo.shutdownNow()
        super.onDestroy()
    }

    // ---------- cores ----------

    private fun aplicarCores() {
        val noturno = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        pal = Ajustes.paleta(aj.tema, noturno, aj.destaque)
        raiz.setBackgroundColor(pal.fundo)
        painel.setBackgroundColor(pal.fundo)
        painelTitulo.setTextColor(pal.texto)
        painelDica.setTextColor(pal.fraco)
        onda.cor = pal.destaque
        onda.corErro = pal.erro
        window?.window?.navigationBarColor = pal.fundo
    }

    // ---------- painel de voz ----------

    private fun montarPainel(): View {
        painelTitulo = TextView(this).apply {
            textSize = 17f
            gravity = Gravity.CENTER
            setTypeface(null, Typeface.BOLD)
        }
        onda = OndaView(this)
        painelDica = TextView(this).apply { textSize = 12.5f; gravity = Gravity.CENTER }
        painel = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            visibility = View.GONE
            isClickable = true // não deixa o toque cair nas teclas cobertas
            setOnClickListener { if (!gravador.ativo && !transcrevendo) esconderPainel() }
            addView(painelTitulo, LinearLayout.LayoutParams(-1, -2))
            addView(onda, LinearLayout.LayoutParams(-1, (72 * dp).toInt()).apply {
                setMargins((28 * dp).toInt(), (10 * dp).toInt(), (28 * dp).toInt(), (10 * dp).toInt())
            })
            addView(painelDica, LinearLayout.LayoutParams(-1, -2))
        }
        return painel
    }

    private fun mostrarPainel(titulo: String, dica: String, estado: OndaView.Estado, someEmMs: Long = 0) {
        principal.removeCallbacks(esconderPainelDepois)
        painelTitulo.text = titulo
        painelTitulo.setTextColor(if (estado == OndaView.Estado.ERRO) pal.erro else pal.texto)
        painelDica.text = dica
        painelDica.visibility = if (dica.isEmpty()) View.GONE else View.VISIBLE
        onda.definir(estado)
        painel.animate().cancel()
        if (painel.visibility != View.VISIBLE) {
            painel.visibility = View.VISIBLE
            painel.alpha = 0f
            painel.translationY = 14 * dp
        }
        painel.animate().alpha(1f).translationY(0f).setDuration(180).setInterpolator(DecelerateInterpolator()).start()
        if (someEmMs > 0) principal.postDelayed(esconderPainelDepois, someEmMs)
    }

    private fun esconderPainel(imediato: Boolean = false) {
        principal.removeCallbacks(esconderPainelDepois)
        if (painel.visibility != View.VISIBLE) return
        painel.animate().cancel()
        if (imediato) {
            painel.visibility = View.GONE
            onda.definir(OndaView.Estado.PARADA)
            return
        }
        painel.animate().alpha(0f).translationY(8 * dp).setDuration(160).withEndAction {
            painel.visibility = View.GONE
            onda.definir(OndaView.Estado.PARADA)
        }.start()
    }

    private fun avisar(texto: String, erro: Boolean = false) =
        mostrarPainel(texto, "", if (erro) OndaView.Estado.ERRO else OndaView.Estado.PARADA, 2600)

    // ---------- ditado ----------

    @SuppressLint("ClickableViewAccessibility")
    private fun aoTocarMic(v: View, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> comecar(v)
            MotionEvent.ACTION_MOVE -> arrastar(v, e)
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
            return avisar(e.message ?: "Microfone indisponível.", erro = true)
        }
        toque(v, forte = true)
        v.isPressed = true
        cancelando = false
        inicioGravacao = SystemClock.elapsedRealtime()
        ultimaVoz = inicioGravacao
        mic.background = pilula(escurecer(pal.destaque))
        mostrarPainel("Ouvindo · 0:00", "Solte para enviar  ·  arraste para cima para cancelar", OndaView.Estado.REC)
        principal.post(relogio)
    }

    /** Arrastar o dedo para cima, sobre o painel, cancela o ditado (como o áudio do WhatsApp). */
    private fun arrastar(v: View, e: MotionEvent) {
        if (!gravador.ativo) return
        val fora = e.y < -v.height * 0.6f
        if (fora == cancelando) return
        cancelando = fora
        toque(v)
        if (fora) {
            mostrarPainel("Solte para cancelar", "O áudio será descartado", OndaView.Estado.ERRO)
        } else {
            mostrarPainel("Ouvindo · ${Regras.cronometro(SystemClock.elapsedRealtime() - inicioGravacao)}",
                "Solte para enviar  ·  arraste para cima para cancelar", OndaView.Estado.REC)
        }
    }

    private fun parar(v: View) {
        if (!gravador.ativo) return
        val g = gravador.parar()
        principal.removeCallbacks(relogio)
        v.isPressed = false
        mic.background = pilula(pal.destaque)
        mic.animate().scaleX(1f).scaleY(1f).setDuration(140).start()
        if (cancelando) {
            cancelando = false
            return mostrarPainel("Ditado cancelado", "", OndaView.Estado.PARADA, 900)
        }
        when (Regras.decidir(g.pcm, g.duracaoS)) {
            Regras.Decisao.CURTO -> avisar("Segure o microfone enquanto fala.")
            Regras.Decisao.SILENCIO -> avisar("Não ouvi voz.")
            Regras.Decisao.ENVIAR -> enviar(v, g.pcm)
        }
    }

    private fun enviar(v: View, pcm: ByteArray) {
        transcrevendo = true
        toque(v)
        mostrarPainel("Transcrevendo…", "", OndaView.Estado.PROC)
        val chave = Cofre(this).ler() // relida a cada ditado: troca nas configurações vale na hora
        val prompt = Vocabulario(this).prompt()
        fundo.execute {
            val resultado = runCatching { Groq.transcrever(chave, Regras.wav(pcm), prompt) }
            principal.post {
                transcrevendo = false
                resultado.onSuccess { texto ->
                    if (texto.isNotEmpty()) currentInputConnection?.commitText("$texto ", 1)
                    esconderPainel()
                }.onFailure { erro ->
                    toque(v, forte = true)
                    mostrarPainel(if (erro is Groq.Erro) erro.message!! else "Sem conexão com a Groq.",
                        "Toque para fechar", OndaView.Estado.ERRO, 4000)
                }
            }
        }
    }

    // ---------- teclas ----------

    @SuppressLint("ClickableViewAccessibility")
    private fun montar() {
        superiores.removeAllViews()
        inferior.removeAllViews()
        letras.clear()
        shiftTecla = null
        esconderVariantes()
        val linhasLetras = if (aj.linhaNumeros) 5 else 4
        val linhas = if (simbolos) 4 else linhasLetras
        val alt = (48 * dp * aj.altura * linhasLetras / linhas).toInt() // altura total igual nas duas camadas
        val m = (2.5f * dp).toInt()
        fun linha(itens: List<Pair<View, Float>>, recuo: Float = 0f, destino: LinearLayout? = null) {
            val l = destino ?: LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; weightSum = 10f }
            if (recuo > 0) l.addView(View(this), LinearLayout.LayoutParams(0, alt, recuo))
            itens.forEach { (v, peso) ->
                l.addView(v, LinearLayout.LayoutParams(0, alt, peso).apply { setMargins(m, m, m, m) })
            }
            if (recuo > 0) l.addView(View(this), LinearLayout.LayoutParams(0, alt, recuo))
            if (destino == null) superiores.addView(l)
        }
        fun texto(fileira: String, registrar: Boolean = false, numeros: Boolean = false) =
            fileira.mapIndexed { i, c ->
                simbolo(c.toString(), registrar, extra = if (numeros) Teclas.NUMEROS[i].toString() else null) to 1f
            }

        val apagar = icone(R.drawable.ic_apagar, "Apagar").also { a ->
            repetirAoSegurar(a) { esconderVariantes(); sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL) }
        }
        if (simbolos) {
            linha(texto(Teclas.NUMEROS))
            linha(texto(Teclas.SIMBOLOS[0]))
            linha(texto(Teclas.SIMBOLOS[1]) + (apagar to 2f))
        } else {
            if (aj.linhaNumeros) linha(texto(Teclas.NUMEROS))
            // sem a fileira de números, segurar q…p digita 1…0 (como o Gboard)
            linha(texto(Teclas.LETRAS[0], true, numeros = !aj.linhaNumeros))
            linha(texto(Teclas.LETRAS[1], true), recuo = 0.5f)
            val sh = icone(R.drawable.ic_shift, "Maiúscula").apply { setOnClickListener { toque(it); alternarShift() } }
            shiftTecla = sh
            linha(listOf<Pair<View, Float>>(sh to 1.5f) + texto(Teclas.LETRAS[2], true) + (apagar to 1.5f))
            atualizarShift()
        }

        val camada = especial(if (simbolos) "ABC" else "?123", "Trocar letras e símbolos") {
            simbolos = !simbolos; montar(); autoMaiuscula()
        }.apply { textSize = 15f }
        mic = ImageView(this).apply {
            setImageResource(R.drawable.ic_mic)
            imageTintList = ColorStateList.valueOf(Color.WHITE)
            scaleType = ImageView.ScaleType.CENTER
            contentDescription = "Segure para falar"
            background = pilula(pal.destaque)
            alpha = if (campoSenha) 0.35f else 1f
            setOnTouchListener { v, e -> aoTocarMic(v, e) }
        }
        val espaco = TextView(this).apply {
            text = rotuloEspaco()
            textSize = 14f
            gravity = Gravity.CENTER
            setLineSpacing(0f, 0.9f)
            setTextColor(pal.fraco)
            background = fundoTecla(especial = false)
            contentDescription = if (aj.espacoFala) "Espaço; segure para falar" else "Espaço"
            isClickable = true
            setOnTouchListener { v, e -> aoTocarEspaco(v, e) }
        }
        espacoTecla = espaco
        // Segurar o enter abre a escolha de teclado; segurar a vírgula abre a personalização.
        val enter = icone(R.drawable.ic_enter, "Enter; segure para trocar de teclado").apply {
            setOnClickListener { toque(it, AudioManager.FX_KEYPRESS_RETURN); enter() }
            setOnLongClickListener { toque(it); seletorDeTeclado(); true }
        }
        linha(listOf(
            camada to 1.4f, simbolo(",", aoSegurar = { abrirAjustes() }) to 1f, mic to 1.3f,
            espaco to 4f, simbolo(".") to 1f, enter to 1.3f,
        ), destino = inferior)
    }

    /** "Português" e, embaixo, a versão (ou o aviso de versão nova), no lugar da antiga barra de cima. */
    private fun rotuloEspaco(): CharSequence {
        val nova = getSharedPreferences("atualizacao", MODE_PRIVATE).getString("nova", null)
            ?.takeIf { Regras.versaoMaior(it, BuildConfig.VERSION_NAME) }
        val sub = if (nova != null) "Nova versão $nova · segure a vírgula" else "v${BuildConfig.VERSION_NAME} · ${BuildConfig.VERSAO_DATA}"
        val s = SpannableStringBuilder("Português\n")
        val ini = s.length
        s.append(sub)
        s.setSpan(RelativeSizeSpan(0.62f), ini, s.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        val corSub = if (nova != null) pal.destaque else (pal.fraco and 0x00FFFFFF) or (0xA0 shl 24)
        s.setSpan(ForegroundColorSpan(corSub), ini, s.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        return s
    }

    /** Toque no espaço digita espaço; segurar liga o microfone até soltar. */
    @SuppressLint("ClickableViewAccessibility")
    private fun aoTocarEspaco(v: View, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                v.isPressed = true; toque(v, som = AudioManager.FX_KEYPRESS_SPACEBAR); espacoSegurado = false
                if (aj.espacoFala) principal.postDelayed(segurarEspaco, aj.atrasoMs + 100)
            }
            MotionEvent.ACTION_MOVE -> if (espacoSegurado) arrastar(mic, e)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                v.isPressed = false
                principal.removeCallbacks(segurarEspaco)
                if (espacoSegurado) parar(mic) else if (e.actionMasked == MotionEvent.ACTION_UP) espaco()
            }
        }
        return true
    }

    /** Espaço duplo vira ". " (como o Gboard). */
    private fun espaco() {
        val agora = System.currentTimeMillis()
        val antes = currentInputConnection?.getTextBeforeCursor(2, 0)?.toString() ?: ""
        if (aj.pontoDuplo && agora - ultimoEspaco < 500 && antes.length == 2 && antes[1] == ' ' && antes[0].isLetterOrDigit()) {
            currentInputConnection?.deleteSurroundingText(1, 0)
            digitar(". ")
            ultimoEspaco = 0
        } else {
            digitar(" ")
            ultimoEspaco = agora
        }
    }

    private var ultimoEspaco = 0L
    private var espacoSegurado = false
    private val segurarEspaco = Runnable { espacoSegurado = true; comecar(mic) }

    // ---------- balão da tecla (como o Gboard), som e vibração ----------

    private val balaoTexto by lazy {
        TextView(this).apply {
            textSize = 30f
            gravity = Gravity.CENTER_HORIZONTAL or Gravity.TOP
            setPadding(0, (8 * dp).toInt(), 0, 0)
        }
    }
    private val balao by lazy {
        PopupWindow(balaoTexto).apply { isClippingEnabled = false; isTouchable = false; elevation = 6 * dp }
    }

    private fun mostrarBalao(t: TextView) {
        balaoTexto.text = t.text
        balaoTexto.setTextColor(pal.texto)
        balaoTexto.background = pilula(pal.especial)
        val w = t.width + (10 * dp).toInt()
        val h = (t.height * 2.1f).toInt()
        val pos = IntArray(2).also { t.getLocationInWindow(it) }
        val x = maxOf(0, pos[0] - (w - t.width) / 2)
        val y = pos[1] + t.height - h
        if (balao.isShowing) balao.update(x, y, w, h) else {
            balao.width = w; balao.height = h
            balao.showAtLocation(t, Gravity.NO_GRAVITY, x, y)
        }
    }

    private fun esconderBalao() { if (balao.isShowing) balao.dismiss() }

    private val vibrador: Vibrator? by lazy {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager).defaultVibrator
        } else {
            @Suppress("DEPRECATION") getSystemService(Context.VIBRATOR_SERVICE) as Vibrator?
        }
    }
    private val audio by lazy { getSystemService(Context.AUDIO_SERVICE) as AudioManager }

    /** Sem bordas (opção do Gboard), a tecla só aparece quando pressionada. */
    private fun fundoTecla(especial: Boolean) = StateListDrawable().apply {
        addState(intArrayOf(android.R.attr.state_pressed), pilula(pal.pressionada))
        addState(intArrayOf(), if (aj.bordas) pilula(if (especial) pal.especial else pal.tecla) else ColorDrawable(Color.TRANSPARENT))
    }

    /**
     * Tecla de texto. Como no Gboard: segurar abre os acentos acima da tecla,
     * deslizar o dedo escolhe e soltar digita o destacado. O tempo de segurar vem dos ajustes.
     */
    @SuppressLint("ClickableViewAccessibility") // o clique fica para a acessibilidade; o toque é tratado aqui
    private fun simbolo(s: String, registrar: Boolean = false, extra: String? = null, aoSegurar: (() -> Unit)? = null) =
        TextView(this).apply {
            text = s
            textSize = 22f
            gravity = Gravity.CENTER
            setTextColor(pal.texto)
            background = fundoTecla(especial = false)
            isClickable = true
            setOnClickListener { toque(it); digitar(text.toString()) }
            val vars = listOfNotNull(extra) + Teclas.variantes(s[0])
            var segurou = false
            val segurar = Runnable {
                segurou = true
                esconderBalao()
                toque(this)
                if (vars.isNotEmpty()) mostrarVariantes(this, vars) else aoSegurar?.invoke()
            }
            setOnTouchListener { v, e ->
                when (e.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        v.isPressed = true; segurou = false; toque(v)
                        if (aj.balao) mostrarBalao(v as TextView)
                        if (vars.isNotEmpty() || aoSegurar != null) principal.postDelayed(segurar, aj.atrasoMs)
                    }
                    MotionEvent.ACTION_MOVE -> if (acentos.isShowing) destacarAcento(v, e.x)
                    MotionEvent.ACTION_UP -> {
                        v.isPressed = false
                        principal.removeCallbacks(segurar)
                        esconderBalao()
                        if (acentos.isShowing) escolherAcento() else if (!segurou) digitar(text.toString())
                    }
                    MotionEvent.ACTION_CANCEL -> {
                        v.isPressed = false
                        principal.removeCallbacks(segurar)
                        esconderBalao(); esconderVariantes()
                    }
                }
                true
            }
            if (registrar) letras += this
        }

    private fun especial(rotulo: String, descricao: String, acao: () -> Unit) = TextView(this).apply {
        text = rotulo
        textSize = 20f
        gravity = Gravity.CENTER
        setTextColor(pal.texto)
        background = fundoTecla(especial = true)
        contentDescription = descricao
        isClickable = true
        setOnClickListener { toque(it); acao() }
    }

    private fun icone(id: Int, descricao: String) = ImageView(this).apply {
        setImageResource(id)
        imageTintList = ColorStateList.valueOf(pal.texto)
        scaleType = ImageView.ScaleType.CENTER
        contentDescription = descricao
        background = fundoTecla(especial = true)
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
        if (!aj.maiusculaAuto || simbolos || shift == Shift.TRAVADA || !::raiz.isInitialized) return
        val tipo = currentInputEditorInfo?.inputType ?: 0
        val caps = tipo != 0 && (currentInputConnection?.getCursorCapsMode(tipo) ?: 0) != 0
        val novo = if (caps) Shift.UMA else Shift.NAO
        if (novo != shift) { shift = novo; atualizarShift() }
    }

    private fun atualizarShift() {
        val maiuscula = shift != Shift.NAO
        letras.forEach { it.text = Teclas.caixa(it.text.toString(), maiuscula) }
        shiftTecla?.apply {
            setImageResource(when (shift) {
                Shift.NAO -> R.drawable.ic_shift
                Shift.UMA -> R.drawable.ic_shift_ativo
                Shift.TRAVADA -> R.drawable.ic_capslock
            })
            imageTintList = ColorStateList.valueOf(if (shift == Shift.TRAVADA) pal.destaque else pal.texto)
        }
    }

    // ---------- acentos (popup acima da tecla, escolhido deslizando) ----------

    private val acentosLinha by lazy {
        LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            val p = (4 * dp).toInt()
            setPadding(p, p, p, p)
        }
    }
    private val acentos by lazy {
        PopupWindow(acentosLinha).apply { isClippingEnabled = false; isTouchable = false; elevation = 8 * dp }
    }
    private var acentoX = 0 // x da popup na janela do teclado
    private var acentoCelula = 1
    private var acentoSel = 0

    private fun mostrarVariantes(tecla: View, vars: List<String>) {
        acentosLinha.removeAllViews()
        acentosLinha.background = pilula(pal.especial)
        acentoCelula = tecla.width
        val h = (tecla.height * 1.15f).toInt()
        vars.forEach { v ->
            acentosLinha.addView(TextView(this).apply {
                text = Teclas.caixa(v, shift != Shift.NAO)
                textSize = 22f
                gravity = Gravity.CENTER
            }, LinearLayout.LayoutParams(acentoCelula, LinearLayout.LayoutParams.MATCH_PARENT))
        }
        val pad = (8 * dp).toInt()
        val w = acentoCelula * vars.size + pad
        val pos = IntArray(2).also { tecla.getLocationInWindow(it) }
        val largura = tecla.rootView.width
        acentoX = (pos[0] - pad / 2).coerceIn(0, maxOf(0, largura - w)) // começa sobre a tecla, como o Gboard
        val y = maxOf(0, pos[1] - h)
        acentos.width = w; acentos.height = h
        acentos.showAtLocation(tecla, Gravity.NO_GRAVITY, acentoX, y)
        acentoSel = 0
        pintarAcento()
    }

    private fun destacarAcento(tecla: View, xNaTecla: Float) {
        val pos = IntArray(2).also { tecla.getLocationInWindow(it) }
        val x = pos[0] + xNaTecla - acentoX - 4 * dp
        val novo = (x / acentoCelula).toInt().coerceIn(0, acentosLinha.childCount - 1)
        if (novo != acentoSel) { acentoSel = novo; pintarAcento(); toque(tecla) }
    }

    private fun pintarAcento() {
        for (i in 0 until acentosLinha.childCount) {
            val t = acentosLinha.getChildAt(i) as TextView
            val ativo = i == acentoSel
            t.background = if (ativo) pilula(pal.destaque) else null
            t.setTextColor(if (ativo) Color.WHITE else pal.texto)
        }
    }

    private fun escolherAcento() {
        val t = acentosLinha.getChildAt(acentoSel) as? TextView
        esconderVariantes()
        t?.let { digitar(it.text.toString()) }
    }

    private fun esconderVariantes() { if (acentos.isShowing) acentos.dismiss() }

    private fun seletorDeTeclado() =
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showInputMethodPicker()

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
                MotionEvent.ACTION_DOWN -> {
                    view.isPressed = true; toque(view, som = AudioManager.FX_KEYPRESS_DELETE)
                    acao(); principal.postDelayed(repetir, 400)
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> { view.isPressed = false; principal.removeCallbacks(repetir) }
            }
            true
        }
    }

    private fun pilula(c: Int) = GradientDrawable().apply { cornerRadius = 8 * dp; setColor(c) }

    private fun escurecer(c: Int) = Color.rgb((Color.red(c) * 0.7f).toInt(), (Color.green(c) * 0.7f).toInt(), (Color.blue(c) * 0.7f).toInt())

    /** Som e vibração de cada tecla, conforme os ajustes (o KEYBOARD_TAP do sistema é fraco no Xiaomi). */
    @Suppress("UNUSED_PARAMETER")
    private fun toque(v: View, som: Int = AudioManager.FX_KEYPRESS_STANDARD, forte: Boolean = false) {
        if (aj.som) audio.playSoundEffect(som, -1f)
        val vib = vibrador ?: return
        val nivel = if (forte && aj.vibracao != "nao") "forte" else aj.vibracao
        if (nivel == "nao") return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            vib.vibrate(VibrationEffect.createPredefined(when (nivel) {
                "fraca" -> VibrationEffect.EFFECT_TICK
                "forte" -> VibrationEffect.EFFECT_HEAVY_CLICK
                else -> VibrationEffect.EFFECT_CLICK
            }))
        } else {
            val ms = when (nivel) { "fraca" -> 10L; "forte" -> 30L; else -> 18L }
            vib.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
        }
    }

    // ---------- configurações e atualização ----------

    private fun abrirConfiguracoes(motivo: String? = null) {
        motivo?.let { avisar(it, erro = true) }
        startActivity(Intent(this, ConfigActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    private fun abrirAjustes() =
        startActivity(Intent(this, AjustesActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))

    private fun procurarAtualizacaoSeVencido() {
        if (Atualizador.ehDev) return
        val prefs = getSharedPreferences("atualizacao", MODE_PRIVATE)
        val agora = System.currentTimeMillis()
        if (agora - prefs.getLong("ultima", 0) < Atualizador.INTERVALO_MS) return
        prefs.edit().putLong("ultima", agora).apply()
        fundo.execute {
            runCatching { Atualizador.procurar() }.onSuccess { nova ->
                prefs.edit().putString("nova", nova?.versao).apply()
                principal.post { espacoTecla?.text = rotuloEspaco() }
            }
        }
    }
}
