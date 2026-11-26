package com.renantalk

/** Fileiras do teclado (cópia do Gboard do Renan) e acentos ao segurar a tecla. */
object Teclas {
    const val NUMEROS = "1234567890"
    val LETRAS = listOf("qwertyuiop", "asdfghjkl", "zxcvbnm")
    val SIMBOLOS = listOf("@#\$_&-+()/", "%*\"':;!?")

    private val VARIANTES = mapOf(
        'a' to "áàâãä", 'e' to "éêè", 'i' to "íî", 'o' to "óôõò", 'u' to "úü",
        'c' to "ç", 'n' to "ñ", '.' to ",?!…:;", '-' to "–—", '$' to "€£",
    )

    fun variantes(tecla: Char): List<String> =
        VARIANTES[tecla.lowercaseChar()]?.map { it.toString() } ?: emptyList()

    fun caixa(s: String, maiuscula: Boolean) = if (maiuscula) s.uppercase() else s.lowercase()
}
