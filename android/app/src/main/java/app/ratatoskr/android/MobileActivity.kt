package app.ratatoskr.android

import android.content.res.ColorStateList
import android.graphics.Color
import android.os.Bundle
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.button.MaterialButton

abstract class MobileActivity : AppCompatActivity() {
    fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
    val dark get() = resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES
    val ink get() = Color.parseColor(if (dark) "#EEE5D7" else "#302D28")
    val paper get() = Color.parseColor(if (dark) "#131820" else "#EAE5DC")
    val surface get() = Color.parseColor(if (dark) "#202731" else "#F4F0E8")
    val accent get() = Color.parseColor(when (MobilePreferences(this).brand) {
        "midnight-arcane" -> if (dark) "#C3A0EB" else "#673E9D"
        "forest-rune" -> if (dark) "#BAD47D" else "#526B1C"
        "frost-byte" -> if (dark) "#90D5F0" else "#14628D"
        else -> if (dark) "#EBBA75" else "#955311"
    })
    override fun onCreate(savedInstanceState: Bundle?) {
        val prefs = MobilePreferences(this)
        val mode = when (prefs.mode) { "dark" -> AppCompatDelegate.MODE_NIGHT_YES; "light" -> AppCompatDelegate.MODE_NIGHT_NO; else -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM }
        if (AppCompatDelegate.getDefaultNightMode() != mode) AppCompatDelegate.setDefaultNightMode(mode)
        if (prefs.language.isNotEmpty() && AppCompatDelegate.getApplicationLocales().toLanguageTags() != prefs.language)
            AppCompatDelegate.setApplicationLocales(LocaleListCompat.forLanguageTags(prefs.language))
        super.onCreate(savedInstanceState)
    }
    fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    fun label(value: String, size: Float = 16f) = TextView(this).apply { text = value; textSize = size; setTextColor(ink); setPadding(0, dp(8), 0, dp(8)) }
    fun button(value: String, action: () -> Unit) = MaterialButton(this).apply {
        text = value; minHeight = dp(48); cornerRadius = dp(12)
        backgroundTintList = ColorStateList.valueOf(surface); setTextColor(accent)
        setOnClickListener { action() }
    }
    fun insets(view: android.view.View) {
        ViewCompat.setOnApplyWindowInsetsListener(view) { target, inset ->
            val bars = inset.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            target.setPadding(dp(20) + bars.left, dp(12) + bars.top, dp(20) + bars.right, dp(16) + bars.bottom)
            inset
        }
    }
}
