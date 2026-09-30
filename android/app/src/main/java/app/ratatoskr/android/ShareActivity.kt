package app.ratatoskr.android

import android.Manifest
import android.content.ClipboardManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The small dialog that appears when a link is shared to Ratatosk (or when
 * the Quick Settings tile is tapped with a link copied): it reads the link,
 * offers the qualities the video has, and hands the choice to the download
 * service. The dialog then closes so the person is back where they were.
 */
class ShareActivity : AppCompatActivity() {
    private lateinit var box: LinearLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(48, 40, 48, 32)
        }
        setContentView(box)

        if (Build_needsNotificationPermission()) {
            ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }

        val shared = intent?.getStringExtra(android.content.Intent.EXTRA_TEXT)
        val text = shared ?: if (intent?.getBooleanExtra(EXTRA_FROM_CLIPBOARD, false) == true) clipboardText() else null
        val url = LinkUtils.extractUrl(text)
        when {
            url == null -> finishWith(R.string.no_link)
            !LinkUtils.isPublicHttpUrl(url) -> finishWith(R.string.bad_link)
            else -> check(url)
        }
    }

    private fun Build_needsNotificationPermission() =
        android.os.Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED

    private fun clipboardText(): String? {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        return clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
    }

    private fun check(url: String) {
        showBusy(getString(R.string.checking))
        lifecycleScope.launch {
            val info = try {
                withContext(Dispatchers.IO) { Engine.probe(this@ShareActivity, url) }
            } catch (e: Exception) {
                finishWith(R.string.cannot_read)
                return@launch
            }
            showChoices(url, info)
        }
    }

    private fun showBusy(message: String) {
        box.removeAllViews()
        box.addView(TextView(this).apply { text = message; textSize = 16f })
        box.addView(ProgressBar(this).apply { isIndeterminate = true })
    }

    private fun showChoices(url: String, info: LinkInfo) {
        box.removeAllViews()
        box.addView(TextView(this).apply {
            text = info.title.ifEmpty { getString(R.string.choose_quality) }
            textSize = 16f
            maxLines = 3
        })
        box.addView(TextView(this).apply { text = getString(R.string.choose_quality); setPadding(0, 16, 0, 8) })

        fun choice(label: String, height: Int?, audio: Boolean) {
            box.addView(Button(this).apply {
                text = label
                setOnClickListener {
                    DownloadService.start(this@ShareActivity, url, height, audio, info.title)
                    finish()
                }
            })
        }

        if (info.heights.isNotEmpty()) info.heights.forEach { choice(getString(R.string.video_height, it), it, false) }
        else if (info.hasVideo) choice(getString(R.string.best_quality), null, false)
        choice(getString(R.string.audio_only), null, true)
        box.addView(Button(this).apply {
            text = getString(R.string.cancel)
            setOnClickListener { finish() }
        })
        box.visibility = View.VISIBLE
        box.gravity = Gravity.START
    }

    private fun finishWith(message: Int) {
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
        finish()
    }

    companion object {
        const val EXTRA_FROM_CLIPBOARD = "from_clipboard"
    }
}
