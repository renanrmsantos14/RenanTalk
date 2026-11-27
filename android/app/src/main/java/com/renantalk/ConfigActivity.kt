package com.renantalk

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.ComponentName
import android.content.Context
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
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
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
    private lateinit var listaVocab: Fluxo
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
        if (temFoco) atualizarPassos() // volta das configurações de acessibilidade
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
        val bolha = ComponentName(this, BolhaService::class.java)
        val ligada = Settings.Secure.getString(contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES).orEmpty()
            .split(':').any { ComponentName.unflattenFromString(it) == bolha }
        val microfone = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        passo("Microfone permitido", microfone, "Permitir") {
            requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 1)
        }
        passo("Aba ligada", ligada, "Ligar", "Acessibilidade › Apps baixados › RenanTalk") {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
        }
        passo("Chave Groq cadastrada", cofre.ler().isNotEmpty(), null, "Cole a chave logo abaixo") {}
    }

    override fun onRequestPermissionsResult(codigo: Int, permissoes: Array<out String>, resultados: IntArray) {
        super.onRequestPermissionsResult(codigo, permissoes, resultados)
        atualizarPassos()
        if (resultados.firstOrNull() == PackageManager.PERMISSION_DENIED && !shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO)) {
            // negado de vez: só pelas configurações do sistema
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
        }
    }

    private fun passo(rotulo: String, feito: Boolean, acao: String?, dica: String? = null, aoClicar: () -> Unit) {
        val linha = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = px(48)
        }
        linha.addView(TextView(this).apply { // marca: ✓ azul quando pronto, círculo vazio quando falta
            text = if (feito) "✓" else ""
            textSize = 13f
            gravity = Gravity.CENTER
            setTypeface(null, Typeface.BOLD)
            setTextColor(getColor(R.color.fundo))
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                if (feito) setColor(getColor(R.color.azul)) else setStroke(px(2), getColor(R.color.texto_fraco))
            }
        }, LinearLayout.LayoutParams(px(22), px(22)).apply { marginEnd = px(12) })
        val textos = coluna()
        textos.addView(texto(rotulo, 15f).apply { if (!feito) setTextColor(getColor(R.color.texto)) })
        if (!feito && dica != null) textos.addView(texto(dica, 12f, fraco = true))
        linha.addView(textos, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        if (!feito && acao != null) linha.addView(botao(acao, aoClicar))
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
            typeface = Typeface.DEFAULT // senha usa fonte monoespaçada por padrão
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
        listaVocab = Fluxo(this, px(8))
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
            listaVocab.addView(TextView(this).apply {
                text = "$termo  ✕"
                textSize = 14f
                setTextColor(getColor(R.color.texto))
                setPadding(px(12), px(7), px(10), px(7))
                background = GradientDrawable().apply {
                    cornerRadius = px(16).toFloat()
                    setColor(0x1A3B82F6)
                    setStroke(px(1), 0x553B82F6)
                }
                if (i >= cabem) { // não cabe no limite do prompt: fica de fora
                    paintFlags = paintFlags or Paint.STRIKE_THRU_TEXT_FLAG
                    alpha = 0.5f
                }
                contentDescription = "Remover $termo"
                minHeight = px(36)
                setOnClickListener { vocab.remover(termo); mostrarVocabulario() }
            })
        }
        if (termos.isEmpty()) listaVocab.addView(texto("Nenhum termo ainda.", 13f, fraco = true))
    }

    // ---------- teste ----------

    private fun secaoTeste(): View {
        val caixa = coluna()
        caixa.addView(texto("Toque no campo: a aba azul aparece em cima do teclado. Segure, fale e solte, ou toque para começar e toque de novo para enviar.", 13f, fraco = true))
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
        addView(texto("Segure a aba azul em cima do teclado, fale e solte.", 14f, fraco = true))
        addView(View(this@ConfigActivity).apply {
            background = GradientDrawable(GradientDrawable.Orientation.LEFT_RIGHT,
                intArrayOf(0x002563EB, 0xFF93C5FD.toInt(), 0x002563EB)).apply { cornerRadius = px(2).toFloat() }
        }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, px(3)).apply { topMargin = px(12) })
    }

    private fun cartao(titulo: String, conteudo: View) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { bottomMargin = px(18) }
        addView(texto(titulo.uppercase(), 12f, fraco = true).apply {
            setTypeface(null, Typeface.BOLD)
            letterSpacing = 0.1f
            setPadding(px(4), 0, 0, px(8))
        })
        addView(LinearLayout(this@ConfigActivity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(px(16), px(12), px(16), px(14))
            background = GradientDrawable().apply {
                cornerRadius = px(16).toFloat()
                setColor(getColor(R.color.superficie))
                setStroke(px(1), 0x14FFFFFF)
            }
            addView(conteudo)
        })
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

/** Quebra os filhos em linhas (chips), sem depender de bibliotecas. Só criado por código, nunca por XML. */
@SuppressLint("ViewConstructor")
private class Fluxo(contexto: Context, private val espaco: Int) : ViewGroup(contexto) {
    override fun onMeasure(largura: Int, altura: Int) {
        val max = MeasureSpec.getSize(largura) - paddingLeft - paddingRight
        var x = 0; var y = 0; var linha = 0
        for (i in 0 until childCount) {
            val f = getChildAt(i)
            f.measure(MeasureSpec.makeMeasureSpec(max, MeasureSpec.AT_MOST), MeasureSpec.UNSPECIFIED)
            if (x > 0 && x + f.measuredWidth > max) { x = 0; y += linha + espaco; linha = 0 }
            x += f.measuredWidth + espaco
            linha = maxOf(linha, f.measuredHeight)
        }
        setMeasuredDimension(MeasureSpec.getSize(largura), y + linha + paddingTop + paddingBottom + if (childCount > 0) espaco else 0)
    }

    override fun onLayout(mudou: Boolean, l: Int, t: Int, r: Int, b: Int) {
        val max = r - l - paddingLeft - paddingRight
        var x = 0; var y = espaco; var linha = 0
        for (i in 0 until childCount) {
            val f = getChildAt(i)
            if (x > 0 && x + f.measuredWidth > max) { x = 0; y += linha + espaco; linha = 0 }
            f.layout(paddingLeft + x, paddingTop + y, paddingLeft + x + f.measuredWidth, paddingTop + y + f.measuredHeight)
            x += f.measuredWidth + espaco
            linha = maxOf(linha, f.measuredHeight)
        }
    }
}
