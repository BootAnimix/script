# Import Dependencies

import colorama
import os

# Define Parameters

commands = ("exit", "help", "clear", "compress", "convert", "build", "preview")

clearConsole = lambda: os.system('cls' if os.name in ('nt', 'dos') else 'clear')
def figlet():
    print(colorama.Fore.BLUE +    "  ____              _      _          _           _       ")
    print(colorama.Fore.GREEN +   " | __ )  ___   ___ | |_   / \   _ __ (_)_ __ ___ (_)_  __ ")
    print(colorama.Fore.MAGENTA + " |  _ \ / _ \ / _ \| __| / _ \ | '_ \| | '_ ` _ \| \ \/ / ")
    print(colorama.Fore.YELLOW +  " | |_) | (_) | (_) | |_ / ___ \| | | | | | | | | | |>  <  ")
    print(colorama.Fore.RED +     " |____/ \___/ \___/ \__/_/   \_\_| |_|_|_| |_| |_|_/_/\_\ ")
    print(colorama.Fore.RESET +   " ")

def shell():
    clearConsole()
    figlet()
    while True:
        shellIn = input(" BootAnimix > ").lower().split()
        
        if shellIn == []:
            continue
        else:
            shellIn = shellIn[0]

        if shellIn in commands:
            pass
        else:
            print(colorama.Fore.RED + shellIn + ": Command not found." + colorama.Fore.RESET)

        if shellIn == commands[0]:
            clearConsole()
            break   
        elif shellIn == commands[1]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Avaliable Executables:' + colorama.Style.RESET_ALL)
            print('''
    help - Displays this message
    convert - Converts a video to a png sequence
    compress - Compresses a video/png sequence
    build - Makes a bootanimation
    preview - Renders a png sequence into a video
    exit - Exits the program
    clear - Clears the console
            ''')
        elif shellIn == commands[2]:
            clearConsole()
            figlet()

    return shellIn

# Initiate The process
colorama.init()

print("Welcome to BootAnimix Shell, Type 'help' for more information")
shell()
