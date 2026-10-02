package app.ratatoskr.android

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.view.inputmethod.EditorInfo
import android.webkit.*
import android.widget.*
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AlertDialog

/** A small browser that notices downloads and media on the pages you open, so they can be saved with Ratatoskr.
 * It passes only the link on: no cookies, no history and no passwords are kept for downloads. */
class BrowserActivity : MobileActivity() {
    private lateinit var web: WebView
    private lateinit var address: EditText
    private lateinit var bar: ProgressBar
    private lateinit var found: android.widget.Button
    private val sniffed = linkedSetOf<String>()

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val box = column().apply { setBackgroundColor(paper) }
        insets(box)
        val top = LinearLayout(this).apply { gravity = android.view.Gravity.CENTER_VERTICAL }
        top.addView(icon(R.drawable.ic_close, getString(R.string.cancel)) { finish() })
        address = EditText(this).apply {
            hint = getString(R.string.browser_address); setTextColor(ink); setHintTextColor(muted); maxLines = 1; isSingleLine = true
            inputType = android.text.InputType.TYPE_TEXT_VARIATION_URI; imeOptions = EditorInfo.IME_ACTION_GO
            background = rounded(surface, 14); setPadding(dp(14), dp(10), dp(14), dp(10)); contentDescription = getString(R.string.browser_address)
            setOnEditorActionListener { _, action, _ -> if (action == EditorInfo.IME_ACTION_GO) { open(text.toString()); true } else false }
        }
        top.addView(address, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(top)
        bar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply { max = 100; visibility = android.view.View.GONE; progressTintList = android.content.res.ColorStateList.valueOf(accent) }
        box.addView(bar, LinearLayout.LayoutParams(-1, dp(3)))

        web = WebView(this).apply {
            settings.javaScriptEnabled = true; settings.domStorageEnabled = true
            settings.allowFileAccess = false; settings.allowContentAccess = false; settings.setSupportMultipleWindows(false)
            webViewClient = object : WebViewClient() {
                // Only web pages: intent:, file:, javascript: and the like are never followed.
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = !LinkUtils.isPublicHttpUrl(request.url.toString())
                override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) { address.setText(url); bar.visibility = android.view.View.VISIBLE }
                override fun onPageFinished(view: WebView, url: String) { bar.visibility = android.view.View.GONE }
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                    val url = request.url.toString()
                    if (BrowserUrl.isMedia(url) && LinkUtils.isPublicHttpUrl(url)) runOnUiThread { if (sniffed.size < 30 && sniffed.add(url)) refreshFound() }
                    return null
                }
            }
            webChromeClient = object : WebChromeClient() { override fun onProgressChanged(view: WebView, newProgress: Int) { bar.progress = newProgress } }
            setDownloadListener { url, _, disposition, mime, length -> offerFile(url, URLUtil.guessFileName(url, disposition, mime), length) }
        }
        box.addView(web, LinearLayout.LayoutParams(-1, 0, 1f))

        val bottom = LinearLayout(this)
        found = button(getString(R.string.found_media, 0)) { showFound() }
        bottom.addView(found, LinearLayout.LayoutParams(0, -2, 1f))
        bottom.addView(button(getString(R.string.download_page)) { downloadPage() }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(bottom)
        setContentView(box)
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() { if (web.canGoBack()) web.goBack() else finish() }
        })
        val start = intent.dataString ?: savedInstanceState?.getString("url")
        if (start != null) open(start) else { address.requestFocus() }
    }

    override fun onSaveInstanceState(outState: Bundle) { outState.putString("url", web.url); super.onSaveInstanceState(outState) }
    override fun onDestroy() { web.stopLoading(); web.destroy(); super.onDestroy() }

    private fun open(text: String) {
        val url = BrowserUrl.normalize(text)
        if (url.isEmpty() || !LinkUtils.isPublicHttpUrl(url)) { Toast.makeText(this, R.string.bad_link, Toast.LENGTH_SHORT).show(); return }
        sniffed.clear(); refreshFound()
        web.loadUrl(url)
        web.requestFocus()
    }

    private fun refreshFound() { found.text = getString(R.string.found_media, sniffed.size) }

    private fun offerFile(url: String, name: String, length: Long) {
        if (!LinkUtils.isPublicHttpUrl(url)) return
        val size = if (length > 0) " (${Format.bytes(length)})" else ""
        AlertDialog.Builder(this).setMessage(getString(R.string.download_file_q, name + size))
            .setPositiveButton(R.string.download_now) { _, _ -> DownloadService.start(this, url, null, false, "", "file"); Toast.makeText(this, R.string.browser_added, Toast.LENGTH_SHORT).show() }
            .setNegativeButton(R.string.cancel, null).show()
    }

    private fun showFound() {
        if (sniffed.isEmpty()) { Toast.makeText(this, R.string.nothing_found, Toast.LENGTH_SHORT).show(); return }
        val links = sniffed.toList()
        AlertDialog.Builder(this).setTitle(R.string.found_title)
            .setItems(links.map { it.substringBefore('?').substringAfterLast('/').ifEmpty { it } }.toTypedArray()) { _, index ->
                val url = links[index]
                if (BrowserUrl.isStream(url)) startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, url))
                else { DownloadService.start(this, url, null, false, "", "file"); Toast.makeText(this, R.string.browser_added, Toast.LENGTH_SHORT).show() }
            }.show()
    }

    /** Hands the page's own address to the video engine, for pages whose media is not a plain file. */
    private fun downloadPage() {
        val url = web.url
        if (url == null || !LinkUtils.isPublicHttpUrl(url)) { Toast.makeText(this, R.string.bad_link, Toast.LENGTH_SHORT).show(); return }
        startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, url))
    }
}
