import ctypes
import win32clipboard
import win32api
import win32gui
import time
import traceback
from typing import Deque
from .processor import add_text_to_custom_clipboard

def listen_to_clipboard(cq: Deque[str]):
    # Define constants
    WM_CLIPBOARDUPDATE = 0x031D

    # Define the window procedure function
    def wnd_proc(hwnd, msg, wparam, lparam):
        if msg == WM_CLIPBOARDUPDATE:
            print("Clipboard content changed")
            try:
                win32clipboard.OpenClipboard()
                clipboard_data = win32clipboard.GetClipboardData()
                add_text_to_custom_clipboard(cq, clipboard_data)
                win32clipboard.CloseClipboard()
            except Exception as e:
                print("Error reading clipboard:", e)
                traceback.print_exc()
        return win32gui.DefWindowProc(hwnd, msg, wparam, lparam)

    # Create a simple window to receive messages
    wc = win32gui.WNDCLASS()
    wc.lpfnWndProc = wnd_proc  # Set the window procedure
    wc.lpszClassName = "ClipboardListener"
    wc.hInstance = win32api.GetModuleHandle(None)

    # Register the window class
    class_atom = win32gui.RegisterClass(wc)
    hwnd = win32gui.CreateWindow(class_atom, "Clipboard Listener", 0, 0, 0, 0, 0, 0, 0, wc.hInstance, None)

    # Add the window as a clipboard listener
    ctypes.windll.user32.AddClipboardFormatListener(hwnd)

    # Start the message loop
    print("Listening for clipboard changes...")
    while True:
        win32gui.PumpMessages()
        time.sleep(0.05)
