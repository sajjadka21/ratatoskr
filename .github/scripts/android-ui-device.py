from pathlib import Path
import os
config = Path(os.environ['HOME']) / '.android/avd/test.avd/config.ini'
settings = dict(line.split('=', 1) for line in config.read_text().splitlines() if '=' in line)
settings.update({'hw.lcd.width': '1200', 'hw.lcd.height': '2562', 'hw.lcd.density': '480', 'skin.name': '1200x2562', 'skin.path': '_no_skin'})
config.write_text(''.join(f'{key}={value}\n' for key, value in settings.items()))