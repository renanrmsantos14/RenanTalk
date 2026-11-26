package com.renantalk

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Atualização pelo release do GitHub: lê o último release; se a versão for maior e tiver RenanTalk.apk,
 * baixa e entrega ao instalador do Android (ele pede a confirmação do usuário).
 */
object Atualizador {
    private const val API = "https://api.github.com/repos/renanrmsantos14/RenanTalk/releases/latest"
    private const val PACOTE = "RenanTalk.apk"
    const val INTERVALO_MS = 6 * 3600 * 1000L

    class Novidade(val versao: String, val url: String)

    val ehDev get() = BuildConfig.VERSAO_DATA == "dev"

    /** null = já está na mais recente (ou o release não tem APK). Lança exceção se não conseguir consultar. */
    fun procurar(): Novidade? {
        val con = (URL(API).openConnection() as HttpURLConnection).apply {
            connectTimeout = 10_000
            readTimeout = 15_000
            setRequestProperty("Accept", "application/vnd.github+json")
            setRequestProperty("User-Agent", "renantalk-android/${BuildConfig.VERSION_NAME}")
        }
        val json = try {
            JSONObject(con.inputStream.bufferedReader().use { it.readText() })
        } finally {
            con.disconnect()
        }
        val versao = json.getString("tag_name").removePrefix("v")
        if (!Regras.versaoMaior(versao, BuildConfig.VERSION_NAME)) return null
        val assets = json.getJSONArray("assets")
        for (i in 0 until assets.length()) {
            val a = assets.getJSONObject(i)
            if (a.getString("name") == PACOTE) return Novidade(versao, a.getString("browser_download_url"))
        }
        return null
    }

    /** Baixa direto para uma sessão do PackageInstaller (sem arquivo no armazenamento). Rodar fora da main thread. */
    fun instalar(contexto: Context, novidade: Novidade) {
        val instalador = contexto.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        val id = instalador.createSession(params)
        instalador.openSession(id).use { sessao ->
            val con = URL(novidade.url).openConnection() as HttpURLConnection
            con.instanceFollowRedirects = true
            con.connectTimeout = 15_000
            con.readTimeout = 60_000
            try {
                con.inputStream.use { entrada ->
                    sessao.openWrite(PACOTE, 0, con.contentLengthLong).use { saida ->
                        entrada.copyTo(saida)
                        sessao.fsync(saida)
                    }
                }
            } finally {
                con.disconnect()
            }
            // O resultado volta para a ConfigActivity, que abre a confirmação do sistema.
            // A ConfigActivity é exportada; o código aleatório impede outro app de forjar este resultado.
            val codigo = java.util.UUID.randomUUID().toString()
            contexto.getSharedPreferences("atualizacao", Context.MODE_PRIVATE).edit().putString(EXTRA_CODIGO, codigo).apply()
            val volta = Intent(contexto, ConfigActivity::class.java).setAction(ACAO_RESULTADO)
                .putExtra(EXTRA_CODIGO, codigo).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            val pendente = PendingIntent.getActivity(
                contexto, id, volta, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
            )
            sessao.commit(pendente.intentSender)
        }
    }

    const val ACAO_RESULTADO = "com.renantalk.RESULTADO_INSTALACAO"
    const val EXTRA_CODIGO = "codigo"

    /** true só para resultados da sessão que este app abriu (o código vale para todos os status dela). */
    fun resultadoLegitimo(contexto: Context, intent: Intent): Boolean {
        val prefs = contexto.getSharedPreferences("atualizacao", Context.MODE_PRIVATE)
        val esperado = prefs.getString(EXTRA_CODIGO, null) ?: return false
        return esperado == intent.getStringExtra(EXTRA_CODIGO)
    }
}
