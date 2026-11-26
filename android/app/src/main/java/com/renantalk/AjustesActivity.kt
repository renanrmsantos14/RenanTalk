package com.renantalk

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView

/**
 * "Personalizar teclado", nos moldes das preferências do Gboard. Cada mudança é gravada na hora
 * e o teclado, se estiver aberto no campo "Experimente", se redesenha na frente do usuário.
 */
class AjustesActivity : Activity() {
    private val dp get() = resources.displayMetrics.density
    private fun px(v: Int) = (v * dp).toInt()
    private val prefs by lazy { Ajustes.prefs(this) }
    private lateinit var coluna: LinearLayout

    override fun onCreate(estado: Bundle?) {
        super.onCreate(estado)
        coluna = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(px(16), px(20), px(16), px(24))
        }
        setContentView(ScrollView(this).apply { addView(coluna); isFillViewport = true })
        desenhar()
    }

    private fun desenhar() {
        coluna.removeAllViews()
        val a = Ajustes.ler(this)
        coluna.addView(texto("Personalizar teclado", 24f).apply { setTypeface(null, Typeface.BOLD) })
        coluna.addView(texto("As mudanças valem na hora. Toque no campo abaixo para ver o teclado enquanto ajusta.", 14f, fraco = true)
            .apply { setPadding(0, px(4), 0, px(12)) })

        coluna.addView(cartao("Experimente", EditText(this).apply {
            hint = "Digite ou segure o microfone…"
            minLines = 2
            gravity = Gravity.TOP
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        }))

        coluna.addView(cartao("Aparência", coluna(
            rotulo("Tema"), escolha("tema", Ajustes.TEMAS, a.tema),
            rotulo("Cor de destaque"), cores(a.destaque),
            rotulo("Altura do teclado"), escolha("altura", Ajustes.ALTURAS, Ajustes.ALTURAS.first { it.first.toFloat() == a.altura }.first),
            chave("Bordas das teclas", "Sem bordas, a tecla só aparece ao ser tocada.", "bordas", a.bordas),
            chave("Fileira de números", "Sem ela, segure q…p para digitar 1…0.", "linhaNumeros", a.linhaNumeros),
        )))

        coluna.addView(cartao("Toque", coluna(
            rotulo("Vibração ao tocar"), escolha("vibracao", Ajustes.VIBRACOES, a.vibracao),
            chave("Som ao tocar", null, "som", a.som),
            chave("Balão ao tocar", "Mostra a letra ampliada acima do dedo.", "balao", a.balao),
            rotulo("Tempo para segurar"), escolha("atraso", Ajustes.ATRASOS, a.atrasoMs.toString()),
        )))

        coluna.addView(cartao("Digitação e voz", coluna(
            chave("Maiúscula automática", "No início de frase.", "maiusculaAuto", a.maiusculaAuto),
            chave("Ponto com espaço duplo", "Dois toques no espaço escrevem \". \".", "pontoDuplo", a.pontoDuplo),
            chave("Segurar o espaço para falar", "Além do botão do microfone.", "espacoFala", a.espacoFala),
        )))

        coluna.addView(cartao("Atalhos", coluna(
            texto("Segure a vírgula para abrir esta tela. Segure o enter para trocar de teclado. " +
                "Ao ditar, arraste o dedo para cima para cancelar.", 13f, fraco = true),
        )))

        coluna.addView(linhaAcoes())
        coluna.addView(texto("v${BuildConfig.VERSION_NAME} · ${BuildConfig.VERSAO_DATA}", 11f, fraco = true).apply {
            gravity = Gravity.END
            setPadding(0, px(16), 0, 0)
        })
    }

    private fun linhaAcoes() = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        addView(acao("Chave, vocabulário e atualização") {
            startActivity(Intent(this@AjustesActivity, ConfigActivity::class.java))
        }, LinearLayout.LayoutParams(0, -2, 1f))
        addView(acao("Restaurar padrão") { prefs.edit().clear().apply(); desenhar() },
            LinearLayout.LayoutParams(-2, -2).apply { leftMargin = px(8) })
    }

    // ---------- controles ----------

    /** Pílulas lado a lado (rolam se não couberem); a escolhida fica preenchida. Repinta sem redesenhar a tela. */
    private fun escolha(chave: String, opcoes: List<Pair<String, String>>, atual: String): View {
        val linha = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; setPadding(0, px(4), 0, px(8)) }
        val pilulas = opcoes.map { (valor, rotulo) ->
            TextView(this).apply {
                text = rotulo
                tag = valor
                textSize = 14f
                gravity = Gravity.CENTER
                minHeight = px(40)
                setPadding(px(16), 0, px(16), 0)
            }.also { linha.addView(it, LinearLayout.LayoutParams(-2, -2).apply { rightMargin = px(8) }) }
        }
        fun pintar(escolhido: String) = pilulas.forEach { p ->
            val ativo = p.tag == escolhido
            p.setTextColor(if (ativo) Color.WHITE else getColor(R.color.texto))
            p.background = GradientDrawable().apply {
                cornerRadius = px(20).toFloat()
                if (ativo) setColor(getColor(R.color.azul)) else setStroke(px(1), getColor(R.color.texto_fraco))
            }
            p.contentDescription = "${p.text}${if (ativo) ", selecionado" else ""}"
        }
        pilulas.forEach { p -> p.setOnClickListener { prefs.edit().putString(chave, p.tag as String).apply(); pintar(p.tag as String) } }
        pintar(atual)
        return HorizontalScrollView(this).apply { isHorizontalScrollBarEnabled = false; addView(linha) }
    }

    private fun cores(atual: String): View {
        val linha = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; setPadding(0, px(4), 0, px(8)) }
        val bolas = Ajustes.DESTAQUES.map { (nome, _) ->
            TextView(this).apply {
                tag = nome
                textSize = 18f
                gravity = Gravity.CENTER
                setTextColor(Color.WHITE)
            }.also { linha.addView(it, LinearLayout.LayoutParams(px(44), px(44)).apply { rightMargin = px(12) }) }
        }
        fun pintar(escolhida: String) = bolas.forEachIndexed { i, b ->
            val ativo = b.tag == escolhida
            b.text = if (ativo) "✓" else ""
            b.background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(Ajustes.DESTAQUES[i].second)
                if (ativo) setStroke(px(3), getColor(R.color.texto))
            }
            b.contentDescription = "Cor ${b.tag}${if (ativo) ", selecionada" else ""}"
        }
        bolas.forEach { b -> b.setOnClickListener { prefs.edit().putString("destaque", b.tag as String).apply(); pintar(b.tag as String) } }
        pintar(atual)
        return linha
    }

    @Suppress("DEPRECATION") // Switch simples basta; o SwitchMaterial exigiria a biblioteca Material
    private fun chave(titulo: String, detalhe: String?, chave: String, ligado: Boolean) = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = px(56)
        val textos = coluna(texto(titulo, 15f), *listOfNotNull(detalhe?.let { texto(it, 12.5f, fraco = true) }).toTypedArray())
        addView(textos, LinearLayout.LayoutParams(0, -2, 1f))
        val sw = Switch(this@AjustesActivity).apply {
            isChecked = ligado
            contentDescription = titulo
            setOnCheckedChangeListener { _, v -> prefs.edit().putBoolean(chave, v).apply() }
        }
        addView(sw)
        setOnClickListener { sw.toggle() }
    }

    // ---------- peças visuais ----------

    private fun cartao(titulo: String, conteudo: View) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(16), px(14), px(16), px(10))
        background = GradientDrawable().apply { cornerRadius = px(16).toFloat(); setColor(getColor(R.color.superficie)) }
        layoutParams = LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = px(12) }
        addView(texto(titulo, 17f).apply { setTypeface(null, Typeface.BOLD); setPadding(0, 0, 0, px(6)) })
        addView(conteudo)
    }

    private fun coluna(vararg filhos: View) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        filhos.forEach { addView(it) }
    }

    private fun rotulo(t: String) = texto(t, 14f, fraco = true).apply { setPadding(0, px(8), 0, 0) }

    private fun texto(t: String, tamanho: Float, fraco: Boolean = false) = TextView(this).apply {
        text = t
        textSize = tamanho
        setTextColor(getColor(if (fraco) R.color.texto_fraco else R.color.texto))
    }

    private fun acao(t: String, aoClicar: () -> Unit) = TextView(this).apply {
        text = t
        textSize = 14f
        gravity = Gravity.CENTER
        minHeight = px(48)
        setPadding(px(14), 0, px(14), 0)
        setTextColor(getColor(R.color.azul))
        setTypeface(null, Typeface.BOLD)
        background = GradientDrawable().apply { cornerRadius = px(24).toFloat(); setStroke(px(1), getColor(R.color.azul)) }
        setOnClickListener { aoClicar() }
    }
}
