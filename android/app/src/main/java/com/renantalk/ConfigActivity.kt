package com.renantalk

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import java.util.concurrent.Executors

/** Configurações: primeiros passos, chave Groq, vocabulário, teste, atualização e versão. */
class ConfigActivity : Activity() {
    private val principal = Handler(Looper.getMainLooper())
    private val fundo = Executors.newSingleThreadExecutor()
    private lateinit var cofre: Cofre
    private lateinit var vocab: Vocabulario
    private lateinit var passos: LinearLayout
    private lateinit var chaveStatus: TextView
    private lateinit var listaVocab: LinearLayout
    private lateinit var contador: TextView
    private lateinit var atualizacaoStatus: TextView
    private lateinit var atualizacaoBotao: Button
    private var novidade: Atualizador.Novidade? = null
    private val dp get() = resources.displayMetrics.density
    private fun px(v: Int) = (v * dp).toInt()

    override fun onCreate(estado: Bundle?) {
        super.onCreate(estado)
        cofre = Cofre(this)
        vocab = Vocabulario(this)
        val coluna = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(px(16), px(20), px(16), px(32))
        }
        coluna.addView(cabecalho())
        passos = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        coluna.addView(cartao("Primeiros passos", passos))
        coluna.addView(cartao("Chave Groq", secaoChave()))
        coluna.addView(cartao("Vocabulário", secaoVocabulario()))
        coluna.addView(cartao("Testar", secaoTeste()))
        coluna.addView(cartao("Teclado", coluna().apply {
            addView(texto("Tema, cores, altura, vibração, som, números e atalhos, como no Gboard.", 13f, fraco = true))
            addView(botao("Personalizar teclado") { startActivity(Intent(this@ConfigActivity, AjustesActivity::class.java)) })
        }))
        coluna.addView(cartao("Atualização", secaoAtualizacao()))
        coluna.addView(texto("O áudio de cada ditado vai só para a Groq transcrever; nada é gravado no aparelho. " +
            "A chave fica cifrada pelo Android Keystore e não sai do celular.", 12f, fraco = true))
        setContentView(ScrollView(this).apply { addView(coluna); isFillViewport = true })
        tratarResultadoInstalacao(intent)
    }

    override fun onResume() {
        super.onResume()
        atualizarPassos()
    }

    override fun onWindowFocusChanged(temFoco: Boolean) {
        super.onWindowFocusChanged(temFoco)
        if (temFoco) atualizarPassos() // volta do seletor de teclado
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        tratarResultadoInstalacao(intent)
    }

    override fun onDestroy() {
        fundo.shutdownNow()
        super.onDestroy()
    }

    // ---------- primeiros passos ----------

    private fun atualizarPassos() {
        passos.removeAllViews()
        val id = ComponentName(this, TecladoService::class.java).flattenToShortString()
        val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
        val ativado = imm.enabledInputMethodList.any { it.packageName == packageName }
        val emUso = Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
            ?.let { ComponentName.unflattenFromString(it)?.flattenToShortString() } == id
        val microfone = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        passo("1. Permitir o microfone", microfone, "Permitir") {
            requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 1)
        }
        passo("2. Ativar o teclado RenanTalk", ativado, "Ativar") {
            startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
        }
        passo("3. Usar o RenanTalk agora", emUso, "Escolher") { imm.showInputMethodPicker() }
        passo("4. Cadastrar a chave Groq", cofre.ler().isNotEmpty(), null) {}
    }

    override fun onRequestPermissionsResult(codigo: Int, permissoes: Array<out String>, resultados: IntArray) {
        super.onRequestPermissionsResult(codigo, permissoes, resultados)
        atualizarPassos()
        if (resultados.firstOrNull() == PackageManager.PERMISSION_DENIED && !shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO)) {
            // negado de vez: só pelas configurações do sistema
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
        }
    }

    private fun passo(rotulo: String, feito: Boolean, acao: String?, aoClicar: () -> Unit) {
        val linha = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = px(48)
        }
        linha.addView(texto(rotulo, 15f), LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        if (feito) {
            linha.addView(texto("✓ pronto", 14f).apply { setTextColor(getColor(R.color.azul)); setTypeface(null, Typeface.BOLD) })
        } else if (acao != null) {
            linha.addView(botao(acao, aoClicar))
        } else {
            linha.addView(texto("pendente", 14f, fraco = true))
        }
        passos.addView(linha)
    }

    // ---------- chave ----------

    private fun secaoChave(): View {
        val caixa = coluna()
        chaveStatus = texto("", 13f, fraco = true)
        mostrarChave()
        val campo = EditText(this).apply {
            hint = "Cole a chave (gsk_…)"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            setSingleLine()
        }
        val salvar = botao("Validar e salvar") {}
        salvar.setOnClickListener {
            val chave = campo.text.toString().trim()
            if (chave.isEmpty()) return@setOnClickListener
            salvar.isEnabled = false
            chaveStatus.text = "Validando na Groq…"
            fundo.execute {
                val erro = Groq.validar(chave)
                if (erro == null) cofre.salvar(chave)
                principal.post {
                    salvar.isEnabled = true
                    if (erro == null) {
                        campo.text.clear()
                        mostrarChave()
                        atualizarPassos()
                        Toast.makeText(this, "Chave salva.", Toast.LENGTH_SHORT).show()
                    } else {
                        chaveStatus.text = erro
                        chaveStatus.setTextColor(getColor(R.color.erro))
                    }
                }
            }
        }
        caixa.addView(chaveStatus)
        caixa.addView(campo)
        caixa.addView(salvar)
        caixa.addView(texto("Crie uma chave gratuita em console.groq.com/keys", 12f, fraco = true))
        return caixa
    }

    private fun mostrarChave() {
        val chave = cofre.ler()
        chaveStatus.setTextColor(getColor(R.color.texto_fraco))
        chaveStatus.text = if (chave.isEmpty()) "Nenhuma chave salva." else "Chave salva, termina em …${chave.takeLast(4)}"
    }

    // ---------- vocabulário ----------

    private fun secaoVocabulario(): View {
        val caixa = coluna()
        caixa.addView(texto("Nomes, siglas e termos que o ditado deve escrever certo. Só neste celular.", 13f, fraco = true))
        val linha = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        val campo = EditText(this).apply {
            hint = "Novo termo"
            setSingleLine()
            imeOptions = EditorInfo.IME_ACTION_DONE
        }
        val adicionar = {
            if (vocab.adicionar(campo.text.toString())) campo.text.clear()
            mostrarVocabulario()
        }
        campo.setOnEditorActionListener { _, _, _ -> adicionar(); true }
        linha.addView(campo, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        linha.addView(botao("Adicionar") { adicionar() })
        caixa.addView(linha)
        contador = texto("", 12f, fraco = true)
        caixa.addView(contador)
        listaVocab = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        caixa.addView(listaVocab)
        mostrarVocabulario()
        return caixa
    }

    private fun mostrarVocabulario() {
        val termos = vocab.ler()
        val cabem = Regras.termosQueCabem(termos)
        contador.text = "${Regras.promptVocabulario(termos.take(cabem)).length} de ${Regras.LIMITE_VOCAB} caracteres" +
            if (cabem < termos.size) " · ${termos.size - cabem} termo(s) riscado(s) ficam de fora" else ""
        listaVocab.removeAllViews()
        termos.forEachIndexed { i, termo ->
            val linha = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
            val rotulo = texto(termo, 15f)
            if (i >= cabem) {
                rotulo.paintFlags = rotulo.paintFlags or Paint.STRIKE_THRU_TEXT_FLAG
                rotulo.alpha = 0.5f
            }
            linha.addView(rotulo, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            linha.addView(TextView(this).apply {
                text = "✕"
                textSize = 16f
                gravity = Gravity.CENTER
                setTextColor(getColor(R.color.texto_fraco))
                contentDescription = "Remover $termo"
                minWidth = px(48)
                minHeight = px(44)
                setOnClickListener { vocab.remover(termo); mostrarVocabulario() }
            })
            listaVocab.addView(linha)
        }
    }

    // ---------- teste ----------

    private fun secaoTeste(): View {
        val caixa = coluna()
        caixa.addView(texto("Toque no campo e segure o microfone ou o espaço para falar; arraste para cima para cancelar. Segure a vírgula para personalizar e o enter para trocar de teclado.", 13f, fraco = true))
        caixa.addView(EditText(this).apply {
            hint = "Fale aqui…"
            minLines = 3
            gravity = Gravity.TOP
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        })
        return caixa
    }

    // ---------- atualização ----------

    private fun secaoAtualizacao(): View {
        val caixa = coluna()
        atualizacaoStatus = texto("Versão instalada: v${BuildConfig.VERSION_NAME} · ${BuildConfig.VERSAO_DATA}", 14f)
        atualizacaoBotao = botao("Procurar atualização") { procurarOuInstalar() }
        caixa.addView(atualizacaoStatus)
        caixa.addView(atualizacaoBotao)
        return caixa
    }

    private fun procurarOuInstalar() {
        novidade?.let { return instalar(it) }
        if (Atualizador.ehDev) {
            atualizacaoStatus.text = "Versão de desenvolvimento: sem atualização automática."
            return
        }
        atualizacaoBotao.isEnabled = false
        atualizacaoStatus.text = "Procurando…"
        fundo.execute {
            val resultado = runCatching { Atualizador.procurar() }
            principal.post {
                atualizacaoBotao.isEnabled = true
                resultado.onSuccess { nova ->
                    getSharedPreferences("atualizacao", MODE_PRIVATE).edit().putString("nova", nova?.versao).apply()
                    novidade = nova
                    if (nova == null) {
                        atualizacaoStatus.text = "Você já está na versão mais recente (v${BuildConfig.VERSION_NAME})."
                    } else {
                        atualizacaoStatus.text = "Nova versão disponível: v${nova.versao}"
                        atualizacaoBotao.text = "Baixar e instalar v${nova.versao}"
                    }
                }.onFailure { atualizacaoStatus.text = "Não consegui procurar atualização agora." }
            }
        }
    }

    private fun instalar(nova: Atualizador.Novidade) {
        if (!packageManager.canRequestPackageInstalls()) {
            Toast.makeText(this, "Permita instalar apps do RenanTalk e volte aqui.", Toast.LENGTH_LONG).show()
            startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
            return
        }
        atualizacaoBotao.isEnabled = false
        atualizacaoStatus.text = "Baixando v${nova.versao}…"
        fundo.execute {
            val resultado = runCatching { Atualizador.instalar(this, nova) }
            principal.post {
                atualizacaoBotao.isEnabled = true
                resultado.onFailure { atualizacaoStatus.text = "Falha ao baixar: ${it.message}" }
            }
        }
    }

    // Lint não enxerga a checagem do código aleatório (Atualizador.resultadoLegitimo), que barra intents forjados.
    @SuppressLint("UnsafeIntentLaunch")
    @Suppress("DEPRECATION")
    private fun tratarResultadoInstalacao(intent: Intent?) {
        if (intent?.action != Atualizador.ACAO_RESULTADO || !Atualizador.resultadoLegitimo(this, intent)) return
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION ->
                intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)?.let { startActivity(it) }
            PackageInstaller.STATUS_SUCCESS -> Unit // o app é reiniciado pelo sistema
            else -> if (::atualizacaoStatus.isInitialized) {
                atualizacaoStatus.text = "Instalação não concluída: ${intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "cancelada"}"
            }
        }
    }

    // ---------- peças visuais ----------

    private fun cabecalho() = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(4), 0, px(4), px(12))
        addView(texto("RenanTalk", 26f).apply { setTypeface(null, Typeface.BOLD) })
        addView(texto("Segure o microfone, fale e solte: o texto entra no campo.", 14f, fraco = true))
        addView(View(this@ConfigActivity).apply {
            background = GradientDrawable(GradientDrawable.Orientation.LEFT_RIGHT,
                intArrayOf(0x002563EB, 0xFF93C5FD.toInt(), 0x002563EB)).apply { cornerRadius = px(2).toFloat() }
        }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, px(3)).apply { topMargin = px(12) })
    }

    private fun cartao(titulo: String, conteudo: View) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(16), px(14), px(16), px(14))
        background = GradientDrawable().apply { cornerRadius = px(16).toFloat(); setColor(getColor(R.color.superficie)) }
        layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { bottomMargin = px(12) }
        addView(texto(titulo, 17f).apply { setTypeface(null, Typeface.BOLD); setPadding(0, 0, 0, px(6)) })
        addView(conteudo)
    }

    private fun coluna() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }

    private fun texto(t: String, tamanho: Float, fraco: Boolean = false) = TextView(this).apply {
        text = t
        textSize = tamanho
        setTextColor(getColor(if (fraco) R.color.texto_fraco else R.color.texto))
        setPadding(0, px(2), 0, px(2))
    }

    private fun botao(t: String, aoClicar: () -> Unit) = Button(this).apply {
        text = t
        isAllCaps = false
        backgroundTintList = ColorStateList.valueOf(getColor(R.color.azul))
        setTextColor(0xFFFFFFFF.toInt())
        setOnClickListener { aoClicar() }
    }
}
