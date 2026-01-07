from pynput.keyboard import Key, Controller

def listen_to_keyboard():
    with keyboard.pressed(Key.ctrl) and (keyboard.pressed('v') or keyboard.pressed('V')):
        pass


if __name__ == '__main__':
    keyboard = Controller()


