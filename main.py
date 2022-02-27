#!/usr/bin/env python3

# Import Dependencies
import colorama
import os

# Define Parameters
commands = {
    # "command" :   "description"
    "help"      :   "Displays this Message",
    "clear"     :   "Clears the Shell",
    "convert"   :   "Converts a video to a png sequence",
    "compress"  :   "Compresses a video/png sequence",
    "build"     :   "Build the Bootanimation",
    "preview"   :   "Renders a png sequence into a video",
    "exit"      :   "Exit the Program"
    }

cmd = list(commands)
val = list(commands.values())

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

        if shellIn in cmd:
            pass
        else:
            print(colorama.Fore.RED + shellIn + ": Command not found." + colorama.Fore.RESET)

        if shellIn == cmd[0]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Avaliable Executables:' + colorama.Style.RESET_ALL)
            for i in range(len(commands)):
                print (f"  {i+1}. {cmd[i]} - {val[i]}")
        elif shellIn == cmd[1]:
            clearConsole()
            figlet()
        elif shellIn == cmd[6]:
            clearConsole()
            break

    return shellIn

# Initiate The process
colorama.init()

print("Welcome to BootAnimix Shell, Type 'help' for more information")
shell()
