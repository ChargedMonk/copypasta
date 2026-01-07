from typing import Any, Deque
from .config import QUEUE_SIZE
import traceback
import win32clipboard
import win32con

def add_text_to_custom_clipboard(cq: Deque[str], content: Any):
    try:
        print(f"New clipboard content: {content}")
        if isinstance(content, str) and content not in cq:
            if len(cq) >= QUEUE_SIZE:
                popped = cq.popleft()
                print(f"Popped {popped} from clipboard!")
            cq.append(content)
    except Exception as e:
        print("Error updating custom clipboard!", e)
        traceback.print_exc()

def modify_default_keyboard(cq: Deque[str]):
    win32clipboard.OpenClipboard()
    format_of_most_recently_copied_item = win32clipboard.EnumClipboardFormats(0)
    if format == win32con.CF_TEXT:
        pass


