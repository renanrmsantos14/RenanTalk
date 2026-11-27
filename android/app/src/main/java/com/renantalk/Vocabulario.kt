package com.renantalk

import android.content.Context

/** Vocabulário do celular, independente dos PCs. Começa com os termos padrão do desktop. */
class Vocabulario(contexto: Context) {
    private val prefs = contexto.getSharedPreferences("vocabulario", Context.MODE_PRIVATE)

    fun ler(): List<String> {
        val texto = prefs.getString(CAMPO, null) ?: return Regras.VOCAB_PADRAO
        return texto.lines().map { it.trim() }.filter { it.isNotEmpty() }
    }

    fun salvar(termos: List<String>) {
        prefs.edit().putString(CAMPO, termos.joinToString("\n")).apply()
    }

    /** Adiciona sem repetir (ignora maiúsculas); devolve false se já existia. */
    fun adicionar(termo: String): Boolean {
        val t = termo.trim()
        val atuais = ler()
        if (t.isEmpty() || atuais.any { it.equals(t, ignoreCase = true) }) return false
        salvar(atuais + t)
        return true
    }

    fun remover(termo: String) = salvar(ler() - termo)

    fun prompt() = Regras.promptVocabulario(ler())

    private companion object {
        const val CAMPO = "termos"
    }
}
