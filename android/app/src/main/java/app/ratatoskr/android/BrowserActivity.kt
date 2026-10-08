package app.ratatoskr.android

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.view.Gravity
import android.view.Menu
import android.view.View
import android.view.inputmethod.EditorInfo
import android.webkit.*
import android.widget.*
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.widget.PopupMenu
import androidx.core.view.isVisible

/** A small multi-tab browser for finding, reviewing, and downloading page media. */
class BrowserActivity : MobileActivity() {
    private class Tab {
        lateinit var web: WebView
        var url = ""
        var title = ""
        val media = linkedSetOf<String>()
    }

    private val tabs = mutableListOf<Tab>()
    private var activeIndex = -1
    private val active: Tab? get() = tabs.getOrNull(activeIndex)
    private lateinit var host: FrameLayout
    private lateinit var home: LinearLayout
    private lateinit var address: EditText
    private lateinit var bar: ProgressBar
    private lateinit var tabButton: TextView
    private lateinit var bookmarkButton: TextView
    private lateinit var mediaButton: TextView
    private lateinit var moreButton: ImageButton
    private lateinit var backButton: ImageButton
    private lateinit var forwardButton: ImageButton

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = column().apply { setBackgroundColor(paper) }
        insets(root)

        val navigation = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
        navigation.addView(icon(R.drawable.ic_close, getString(R.string.cancel), size = 42) { finish() })
        backButton = icon(R.drawable.ic_browser_back, getString(R.string.browser_back), size = 42) { active?.web?.let { if (it.canGoBack()) it.goBack() } }
        navigation.addView(backButton)
        forwardButton = icon(R.drawable.ic_browser_forward, getString(R.string.browser_forward), size = 42) { active?.web?.let { if (it.canGoForward()) it.goForward() } }
        navigation.addView(forwardButton)
        address = EditText(this).apply {
            hint = getString(R.string.browser_address)
            setTextColor(ink); setHintTextColor(muted); textSize = 14f
            maxLines = 1; isSingleLine = true
            inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_URI
            imeOptions = EditorInfo.IME_ACTION_GO
            background = rounded(surface, 16)
            setPadding(dp(14), dp(8), dp(14), dp(8)); minimumHeight = dp(48)
            setSelectAllOnFocus(true)
            setOnEditorActionListener { _, action, _ ->
                if (action == EditorInfo.IME_ACTION_GO || action == EditorInfo.IME_ACTION_DONE) { open(text.toString()); true } else false
            }
        }
        navigation.addView(address, LinearLayout.LayoutParams(0, -2, 1f))
        moreButton = icon(R.drawable.ui_ellipsis_vertical, getString(R.string.browser_menu), size = 42) { showMenu(moreButton) }
        navigation.addView(moreButton)
        root.addView(navigation)

        bar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100; visibility = View.GONE
            progressTintList = android.content.res.ColorStateList.valueOf(accent)
        }
        root.addView(bar, LinearLayout.LayoutParams(-1, dp(3)))

        val actions = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
        tabButton = actionText("", R.string.browser_tabs) { showTabs() }
        bookmarkButton = actionText("", R.string.browser_bookmark) { toggleBookmark() }
        mediaButton = actionText("", R.string.browser_media_found) { showFound() }
        actions.addView(tabButton, LinearLayout.LayoutParams(0, dp(44), 1f))
        actions.addView(bookmarkButton, LinearLayout.LayoutParams(0, dp(44), 1f))
        actions.addView(mediaButton, LinearLayout.LayoutParams(0, dp(44), 1f))
        actions.addView(actionText(getString(R.string.browser_download_page), R.string.browser_download_page) { downloadPage() }, LinearLayout.LayoutParams(0, dp(44), 1f))
        actions.addView(actionText(getString(R.string.browser_reload), R.string.browser_reload) { reload() }, LinearLayout.LayoutParams(0, dp(44), 1f))
        root.addView(actions)

        host = FrameLayout(this)
        home = column().apply {
            gravity = Gravity.CENTER
            setPadding(dp(28), dp(24), dp(28), dp(24))
            addView(TextView(this@BrowserActivity).apply {
                text = getString(R.string.browser_home_title); textSize = 24f; setTextColor(ink); gravity = Gravity.CENTER
            })
            addView(label(getString(R.string.browser_home_hint), 14f).apply { gravity = Gravity.CENTER })
            addView(button(getString(R.string.browser_open_bookmarks)) { showLibrary(bookmarks = true) })
            addView(button(getString(R.string.browser_open_history)) { showLibrary(bookmarks = false) })
        }
        host.addView(home, FrameLayout.LayoutParams(-1, -1))
        root.addView(host, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(root)

        val savedUrls = savedInstanceState?.getStringArrayList(STATE_TABS)
        if (!savedUrls.isNullOrEmpty()) {
            savedUrls.take(MAX_TABS).forEach { createTab(it) }
            activate(savedInstanceState?.getInt(STATE_ACTIVE, 0)?.coerceIn(0, tabs.lastIndex) ?: 0)
        } else {
            val start = intent.dataString?.takeIf { LinkUtils.isPublicHttpUrl(it) }
            createTab(start)
        }
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                val view = active?.web
                if (view?.canGoBack() == true) view.goBack()
                else if (tabs.size > 1) closeTab(activeIndex)
                else finish()
            }
        })
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putStringArrayList(STATE_TABS, ArrayList(tabs.map { it.web.url ?: it.url }))
        outState.putInt(STATE_ACTIVE, activeIndex.coerceAtLeast(0))
        super.onSaveInstanceState(outState)
    }

    override fun onDestroy() {
        tabs.forEach { tab -> runCatching { tab.web.stopLoading(); tab.web.destroy() } }
        tabs.clear()
        super.onDestroy()
    }

    private fun actionText(value: String, contentDescription: Int, fallback: Int = 0, action: () -> Unit) = TextView(this).apply {
        text = value.ifEmpty { if (fallback != 0) getString(fallback, 0) else getString(contentDescription) }
        textSize = 12f; gravity = Gravity.CENTER; setTextColor(accent); this.contentDescription = getString(contentDescription)
        setOnClickListener { action() }; background = rounded(surface, 12)
    }

    private fun createTab(url: String? = null) {
        if (tabs.size >= MAX_TABS) {
            Toast.makeText(this, R.string.browser_tab_limit, Toast.LENGTH_SHORT).show()
            return
        }
        val tab = Tab()
        tab.web = WebView(this).apply {
            layoutParams = FrameLayout.LayoutParams(-1, -1)
            isVerticalScrollBarEnabled = false; isHorizontalScrollBarEnabled = false
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.javaScriptCanOpenWindowsAutomatically = false
            settings.setSupportMultipleWindows(false)
            if (android.os.Build.VERSION.SDK_INT >= 26) settings.safeBrowsingEnabled = true
            settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            visibility = View.GONE
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                    val url = request.url.toString()
                    if (LinkUtils.isPublicHttpUrl(url)) return false
                    Toast.makeText(this@BrowserActivity, R.string.browser_unsupported_link, Toast.LENGTH_SHORT).show()
                    return true
                }
                override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                    tab.url = url
                    if (active === tab) { address.setText(url); bar.progress = 0; bar.visibility = View.VISIBLE; updateControls() }
                }
                override fun onPageFinished(view: WebView, url: String) {
                    tab.url = url
                    if (LinkUtils.isPublicHttpUrl(url)) BrowserStore.recordHistory(this@BrowserActivity, url, tab.title)
                    if (active === tab) { address.setText(url); bar.visibility = View.GONE; updateControls() }
                }
                override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                    if (request.isForMainFrame && active === tab) {
                        bar.visibility = View.GONE
                        Toast.makeText(this@BrowserActivity, R.string.browser_page_failed, Toast.LENGTH_SHORT).show()
                    }
                }
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                    val candidate = request.url.toString()
                    if (BrowserUrl.isMedia(candidate) && LinkUtils.isPublicHttpUrl(candidate)) runOnUiThread {
                        if (tabs.contains(tab) && tab.media.size < MAX_MEDIA_CANDIDATES && tab.media.add(candidate)) updateControls()
                    }
                    return null
                }
            }
            webChromeClient = object : WebChromeClient() {
                override fun onProgressChanged(view: WebView, newProgress: Int) {
                    if (active === tab) { bar.progress = newProgress; bar.visibility = if (newProgress in 1..99) View.VISIBLE else View.GONE }
                }
                override fun onReceivedTitle(view: WebView, title: String?) {
                    tab.title = title?.take(120).orEmpty()
                    if (active === tab) updateControls()
                }
            }
            setDownloadListener { downloadUrl, _, disposition, mime, length ->
                val safeUrl = downloadUrl.takeIf { LinkUtils.isPublicHttpUrl(it) } ?: return@setDownloadListener
                val name = URLUtil.guessFileName(safeUrl, disposition, mime)
                val size = if (length > 0) " · ${Format.bytes(length)}" else ""
                AlertDialog.Builder(this@BrowserActivity).setTitle(R.string.browser_download_file)
                    .setMessage(getString(R.string.download_file_q, name + size))
                    .setPositiveButton(R.string.download_now) { _, _ ->
                        val session = BrowserSession.create(safeUrl, CookieManager.getInstance().getCookie(safeUrl))
                        DownloadService.start(this@BrowserActivity, safeUrl, null, false, name, "file", browserSession = session)
                        Toast.makeText(this@BrowserActivity, R.string.browser_added, Toast.LENGTH_SHORT).show()
                    }.setNegativeButton(R.string.cancel, null).show()
            }
        }
        host.addView(tab.web)
        tabs.add(tab)
        activate(tabs.lastIndex)
        if (!url.isNullOrBlank()) open(url)
    }

    private fun activate(index: Int) {
        if (index !in tabs.indices) return
        activeIndex = index
        tabs.forEachIndexed { i, tab -> tab.web.visibility = if (i == index && tab.url.isNotBlank()) View.VISIBLE else View.GONE }
        home.isVisible = active?.url.isNullOrBlank()
        active?.let { tab -> address.setText(tab.web.url ?: tab.url); bar.visibility = View.GONE }
        updateControls()
    }

    private fun closeTab(index: Int) {
        if (index !in tabs.indices) return
        val wasActive = index == activeIndex
        val tab = tabs.removeAt(index)
        host.removeView(tab.web)
        runCatching { tab.web.stopLoading(); tab.web.destroy() }
        if (tabs.isEmpty()) { activeIndex = -1; createTab(); return }
        if (wasActive) activeIndex = minOf(index, tabs.lastIndex) else if (index < activeIndex) activeIndex--
        activate(activeIndex)
    }

    private fun updateControls() {
        val tab = active
        val view = tab?.web
        backButton.isEnabled = view?.canGoBack() == true
        forwardButton.isEnabled = view?.canGoForward() == true
        backButton.alpha = if (backButton.isEnabled) 1f else .45f
        forwardButton.alpha = if (forwardButton.isEnabled) 1f else .45f
        tabButton.text = getString(R.string.browser_tabs_count, tabs.size)
        bookmarkButton.text = getString(if (tab != null && tab.url.isNotBlank() && BrowserStore.isBookmarked(this, tab.url)) R.string.browser_saved else R.string.browser_bookmark)
        mediaButton.text = getString(R.string.browser_media_count, tab?.media?.size ?: 0)
        home.isVisible = tab?.url.isNullOrBlank()
        tab?.web?.visibility = if (tab?.url.isNullOrBlank()) View.GONE else View.VISIBLE
    }

    private fun open(text: String) {
        val url = BrowserUrl.normalize(text)
        if (url.isEmpty() || !LinkUtils.isPublicHttpUrl(url)) {
            Toast.makeText(this, R.string.bad_link, Toast.LENGTH_SHORT).show()
            return
        }
        val tab = active ?: run { createTab(); active ?: return }
        tab.url = url; tab.media.clear(); tab.title = ""
        address.setText(url); home.isVisible = false; bar.visibility = View.VISIBLE; bar.progress = 0
        updateControls(); tab.web.loadUrl(url); tab.web.requestFocus()
    }

    private fun reload() {
        val view = active?.web ?: return
        if (view.url.isNullOrBlank()) { address.requestFocus(); return }
        if (bar.visibility == View.VISIBLE && bar.progress < 100) view.stopLoading() else view.reload()
    }

    private fun toggleBookmark() {
        val tab = active ?: return
        val url = tab.web.url?.takeIf { LinkUtils.isPublicHttpUrl(it) } ?: return
        val saved = BrowserStore.toggleBookmark(this, url, tab.title)
        Toast.makeText(this, if (saved) R.string.browser_bookmarked else R.string.browser_bookmark_removed, Toast.LENGTH_SHORT).show()
        updateControls()
    }

    private fun showMenu(anchor: View) {
        PopupMenu(this, anchor).apply {
            menu.add(Menu.NONE, MENU_NEW_TAB, Menu.NONE, R.string.browser_new_tab)
            menu.add(Menu.NONE, MENU_BOOKMARKS, Menu.NONE, R.string.browser_open_bookmarks)
            menu.add(Menu.NONE, MENU_HISTORY, Menu.NONE, R.string.browser_open_history)
            menu.add(Menu.NONE, MENU_COPY, Menu.NONE, R.string.copy_link)
            menu.add(Menu.NONE, MENU_SHARE, Menu.NONE, R.string.share_file)
            menu.add(Menu.NONE, MENU_CLEAR_HISTORY, Menu.NONE, R.string.browser_clear_history)
            setOnMenuItemClickListener { item ->
                when (item.itemId) {
                    MENU_NEW_TAB -> createTab()
                    MENU_BOOKMARKS -> showLibrary(true)
                    MENU_HISTORY -> showLibrary(false)
                    MENU_COPY -> copyCurrent()
                    MENU_SHARE -> shareCurrent()
                    MENU_CLEAR_HISTORY -> confirmClearHistory()
                }; true
            }
        }.show()
    }

    private fun copyCurrent() {
        val url = active?.web?.url?.takeIf { LinkUtils.isPublicHttpUrl(it) } ?: return
        (getSystemService(CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("link", url))
        Toast.makeText(this, R.string.link_copied, Toast.LENGTH_SHORT).show()
    }

    private fun shareCurrent() {
        val url = active?.web?.url?.takeIf { LinkUtils.isPublicHttpUrl(it) } ?: return
        startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, url), getString(R.string.share_file)))
    }

    private fun showTabs() {
        val box = column().apply { setPadding(dp(16), dp(8), dp(16), dp(12)); setBackgroundColor(surface) }
        val dialog = AlertDialog.Builder(this).setTitle(R.string.browser_tabs_title).setView(box).create()
        tabs.forEachIndexed { index, tab ->
            val row = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
            row.addView(button(tab.title.ifBlank { tab.url.ifBlank { getString(R.string.browser_new_tab) } }) {
                dialog.dismiss(); activate(index)
            }, LinearLayout.LayoutParams(0, -2, 1f))
            row.addView(icon(R.drawable.ic_close, getString(R.string.browser_close_tab), size = 42) {
                dialog.dismiss(); closeTab(index); showTabs()
            })
            box.addView(row)
        }
        box.addView(button(getString(R.string.browser_new_tab)) { dialog.dismiss(); createTab() })
        dialog.show()
    }

    private fun showLibrary(bookmarks: Boolean) {
        val entries = if (bookmarks) BrowserStore.bookmarks(this) else BrowserStore.history(this)
        val title = if (bookmarks) R.string.browser_open_bookmarks else R.string.browser_open_history
        if (entries.isEmpty()) {
            Toast.makeText(this, if (bookmarks) R.string.browser_bookmarks_empty else R.string.browser_history_empty, Toast.LENGTH_SHORT).show()
            return
        }
        val box = column().apply { setPadding(dp(16), dp(8), dp(16), dp(12)); setBackgroundColor(surface) }
        val dialog = AlertDialog.Builder(this).setTitle(title).setView(box).create()
        entries.take(100).forEach { entry ->
            box.addView(button(entry.title.ifBlank { entry.url }) {
                dialog.dismiss()
                if (active == null) createTab()
                open(entry.url)
            })
        }
        if (bookmarks) box.addView(button(getString(R.string.browser_clear_bookmarks)) {
            dialog.dismiss(); BrowserStore.clearBookmarks(this); Toast.makeText(this, R.string.browser_bookmarks_cleared, Toast.LENGTH_SHORT).show()
        }) else box.addView(button(getString(R.string.browser_clear_history)) {
            dialog.dismiss(); BrowserStore.clearHistory(this); Toast.makeText(this, R.string.browser_history_cleared, Toast.LENGTH_SHORT).show()
        })
        dialog.show()
    }

    private fun confirmClearHistory() {
        AlertDialog.Builder(this).setMessage(R.string.browser_clear_history_q)
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(R.string.browser_clear_history) { _, _ ->
                BrowserStore.clearHistory(this); Toast.makeText(this, R.string.browser_history_cleared, Toast.LENGTH_SHORT).show()
            }.show()
    }

    private fun showFound() {
        val links = active?.media?.toList().orEmpty()
        if (links.isEmpty()) { Toast.makeText(this, R.string.nothing_found, Toast.LENGTH_SHORT).show(); return }
        AlertDialog.Builder(this).setTitle(R.string.found_title)
            .setItems(links.map { it.substringBefore('?').substringAfterLast('/').ifEmpty { it } }.toTypedArray()) { _, index ->
                val url = links[index]
                if (BrowserUrl.isStream(url)) startActivity(mediaIntent(url))
                else {
                    val session = BrowserSession.create(url, CookieManager.getInstance().getCookie(url))
                    DownloadService.start(this, url, null, false, "", "file", browserSession = session)
                    Toast.makeText(this, R.string.browser_added, Toast.LENGTH_SHORT).show()
                }
            }.show()
    }

    /** Sends the current page to the media picker, with a short-lived in-memory page session. */
    private fun downloadPage() {
        val url = active?.web?.url?.takeIf { LinkUtils.isPublicHttpUrl(it) }
        if (url == null) { Toast.makeText(this, R.string.bad_link, Toast.LENGTH_SHORT).show(); return }
        startActivity(mediaIntent(url))
    }

    private fun mediaIntent(url: String): Intent {
        val session = BrowserSession.create(url, CookieManager.getInstance().getCookie(url))
        val token = BrowserSessionHandoff.stage(session)
        return Intent(this, ShareActivity::class.java)
            .putExtra(Intent.EXTRA_TEXT, url)
            .putExtra("choose-media-items", true)
            .putExtra(ShareActivity.EXTRA_BROWSER_SESSION, token)
    }

    companion object {
        private const val STATE_TABS = "browser_tabs"
        private const val STATE_ACTIVE = "browser_active"
        private const val MAX_TABS = 12
        private const val MAX_MEDIA_CANDIDATES = 30
        private const val MENU_NEW_TAB = 1
        private const val MENU_BOOKMARKS = 2
        private const val MENU_HISTORY = 3
        private const val MENU_COPY = 4
        private const val MENU_SHARE = 5
        private const val MENU_CLEAR_HISTORY = 6
    }
}

