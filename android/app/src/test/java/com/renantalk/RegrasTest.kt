package com.renantalk

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RegrasTest {
    private fun pcm(vararg amostras: Int) = ByteArray(amostras.size * 2).also { b ->
        amostras.forEachIndexed { i, v -> b[2 * i] = (v and 0xFF).toByte(); b[2 * i + 1] = (v shr 8).toByte() }
    }

    @Test fun pontuacaoSozinhaNaoEhFala() {
        assertTrue(Regras.semFala("."))
        assertTrue(Regras.semFala(" … "))
        assertTrue(Regras.semFala(""))
        assertFalse(Regras.semFala("Ok."))
        assertFalse(Regras.semFala("2"))
    }

    @Test fun toqueCurtoEhIgnorado() =
        assertEquals(Regras.Decisao.CURTO, Regras.decidir(pcm(20000), 0.39))

    @Test fun silencioEhIgnorado() =
        assertEquals(Regras.Decisao.SILENCIO, Regras.decidir(pcm(100, -799, 500), 2.0))

    @Test fun vozEhEnviada() {
        assertEquals(Regras.Decisao.ENVIAR, Regras.decidir(pcm(100, -800), 0.4))
        assertEquals(Regras.Decisao.ENVIAR, Regras.decidir(pcm(-32768), 1.0))
    }

    @Test fun promptIgualAoDesktop() =
        assertEquals("Betinhos, Dataverse.", Regras.promptVocabulario(listOf("Betinhos", "Dataverse")))

    @Test fun promptVazio() = assertEquals("", Regras.promptVocabulario(emptyList()))

    @Test fun promptCortaSemQuebrarTermo() {
        val termos = List(100) { "termo$it" } // 6-7 caracteres cada
        val prompt = Regras.promptVocabulario(termos)
        assertTrue(prompt.length <= Regras.LIMITE_VOCAB + 1)
        val cabem = Regras.termosQueCabem(termos)
        assertEquals(termos.take(cabem).joinToString(", ") + ".", prompt)
        assertTrue(cabem < termos.size)
    }

    @Test fun padraoCabeInteiro() =
        assertEquals(Regras.VOCAB_PADRAO.size, Regras.termosQueCabem(Regras.VOCAB_PADRAO))

    @Test fun cabecalhoWav() {
        val wav = Regras.wav(pcm(1, 2, 3))
        assertEquals(44 + 6, wav.size)
        assertEquals("RIFF", String(wav, 0, 4))
        assertEquals("WAVE", String(wav, 8, 4))
        assertEquals("data", String(wav, 36, 4))
        assertEquals(16000, (wav[24].toInt() and 0xFF) or ((wav[25].toInt() and 0xFF) shl 8))
    }

    @Test fun comparaVersoes() {
        assertTrue(Regras.versaoMaior("0.0.10", "0.0.9"))
        assertTrue(Regras.versaoMaior("v0.1.0", "0.0.99"))
        assertFalse(Regras.versaoMaior("0.0.8", "0.0.8"))
        assertFalse(Regras.versaoMaior("0.0.7", "0.0.8"))
    }

    @Test fun inserirNoCursor() {
        assertEquals("oi" to 2, Regras.inserir("", -1, -1, "oi"))
        assertEquals("Olá mundo" to 9, Regras.inserir("Olá", 3, 3, "mundo"))
        assertEquals("Olá mundo!" to 9, Regras.inserir("Olá!", 3, 3, "mundo"))
        assertEquals("a b" to 3, Regras.inserir("a ", 2, 2, "b"))
        assertEquals("Olá Renan" to 9, Regras.inserir("Olá XYZ", 4, 7, "Renan")) // troca a seleção
    }

    @Test fun nivelDaLuz() {
        assertEquals(0f, Regras.nivel(ShortArray(4), 4))
        assertEquals(1f, Regras.nivel(ShortArray(4) { 5000 }, 4))
        assertEquals(0f, Regras.nivel(ShortArray(4) { 30 }, 4)) // ruído de fundo não acende
        assertEquals(0.65f, Regras.nivel(ShortArray(4) { 1000 }, 4), 0.03f) // voz normal já acende bem
    }
}
