package com.renantalk

import android.content.Context
import android.content.SharedPreferences

/**
 * Personalização do teclado, nos moldes das preferências do Gboard.
 * Fica num SharedPreferences próprio; o teclado escuta as mudanças e se remonta na hora.
 */
object Ajustes {
    const val ARQUIVO = "teclado"

    val TEMAS = listOf("sistema" to "Sistema", "claro" to "Claro", "escuro" to "Escuro", "preto" to "Preto")
    val DESTAQUES = listOf(
        "azul" to 0xFF3B82F6.toInt(), "verde" to 0xFF10B981.toInt(), "roxo" to 0xFF8B5CF6.toInt(),
        "laranja" to 0xFFF97316.toInt(), "rosa" to 0xFFEC4899.toInt(),
    )
    val ALTURAS = listOf("0.85" to "Baixa", "0.92" to "Média-baixa", "1" to "Normal", "1.1" to "Média-alta", "1.2" to "Alta")
    val VIBRACOES = listOf("nao" to "Desligada", "fraca" to "Fraca", "media" to "Média", "forte" to "Forte")
    val ATRASOS = listOf("200" to "Curto", "300" to "Médio", "450" to "Longo")

    /** Retrato das preferências; lido de novo a cada mudança. */
    data class Teclado(
        val tema: String = "sistema",
        val destaque: String = "azul",
        val altura: Float = 1f,
        val linhaNumeros: Boolean = true,
        val bordas: Boolean = true,
        val balao: Boolean = true,
        val som: Boolean = false,
        val vibracao: String = "media",
        val atrasoMs: Long = 300,
        val maiusculaAuto: Boolean = true,
        val pontoDuplo: Boolean = true,
        val espacoFala: Boolean = true,
    )

    data class Paleta(
        val fundo: Int, val tecla: Int, val especial: Int, val pressionada: Int,
        val texto: Int, val fraco: Int, val destaque: Int, val erro: Int,
    )

    fun prefs(c: Context): SharedPreferences = c.getSharedPreferences(ARQUIVO, Context.MODE_PRIVATE)

    fun ler(c: Context): Teclado {
        val p = prefs(c)
        val d = Teclado()
        return Teclado(
            tema = p.getString("tema", d.tema)!!,
            destaque = p.getString("destaque", d.destaque)!!,
            altura = p.getString("altura", null)?.toFloatOrNull() ?: d.altura,
            linhaNumeros = p.getBoolean("linhaNumeros", d.linhaNumeros),
            bordas = p.getBoolean("bordas", d.bordas),
            balao = p.getBoolean("balao", d.balao),
            som = p.getBoolean("som", d.som),
            vibracao = p.getString("vibracao", d.vibracao)!!,
            atrasoMs = p.getString("atraso", null)?.toLongOrNull() ?: d.atrasoMs,
            maiusculaAuto = p.getBoolean("maiusculaAuto", d.maiusculaAuto),
            pontoDuplo = p.getBoolean("pontoDuplo", d.pontoDuplo),
            espacoFala = p.getBoolean("espacoFala", d.espacoFala),
        )
    }

    /** Cores do teclado. `noturno` = tema escuro do sistema (usado quando o tema é "sistema"). */
    fun paleta(tema: String, noturno: Boolean, destaque: String): Paleta {
        val cor = DESTAQUES.firstOrNull { it.first == destaque }?.second ?: DESTAQUES[0].second
        return when (if (tema == "sistema") (if (noturno) "escuro" else "claro") else tema) {
            "claro" -> Paleta(0xFFE6E9EF.toInt(), 0xFFFFFFFF.toInt(), 0xFFCBD3DE.toInt(), 0xFFAEB8C6.toInt(),
                0xFF0F172A.toInt(), 0xFF64748B.toInt(), cor, 0xFFDC2626.toInt())
            "preto" -> Paleta(0xFF000000.toInt(), 0xFF1A1A1A.toInt(), 0xFF2B2B2B.toInt(), 0xFF3D3D3D.toInt(),
                0xFFF1F5F9.toInt(), 0xFF8A8F98.toInt(), cor, 0xFFF87171.toInt())
            else -> Paleta(0xFF131C2E.toInt(), 0xFF1E293B.toInt(), 0xFF334155.toInt(), 0xFF475569.toInt(),
                0xFFE2E8F0.toInt(), 0xFF94A3B8.toInt(), cor, 0xFFF87171.toInt())
        }
    }
}
