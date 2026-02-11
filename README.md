**Rust-powered LAN file sharing** – fast, private, and under your control.
Take back control of sending and receiving data across devices without relying on
cloud services or slow proprietary apps.
Before **rShare**, sharing large files often meant:
- Clunky email attachments
- Limited services like Opera GX MyFlow (one device pair, auto-deletes files)
- Slow uploads to Google Drive or other clouds

rShare fixes that with a **direct LAN connection** for instant, reliable transfers.

HOW TO USE:

Step 1: 
If it does not exist already, make a folder called "uploads"

    Command prompt, bash, powershell, etc:

    -> /{yourname}/{projects}/rshare/uploads <place it here...>
Step 2: 
You should make your own password. Make a file called PASSWORD.env. Don't share it. 

Example:

    PASSWORD.env

        APP_PASSWORD=password_u_want

Step 3: 
Run this command to make keys. 

(NOTE! you will need openssl to run this command. RUN IT IN THE CORRECT FOLDER.)

Command prompt, bash, powershell, etc:

    -> /{yourname}/{projects}/rshare : 
    
      openssl req -x509 -newkey rsa:4096 -nodes -keyout key.pem -out cert.pem -days 365 -subj '/CN=localhost' 

      This command will make 2 files, One called "cert.pem" and one called "key.pem"


If I wrote the code correctly, Step 2 and 3 are unnecessary. Just be sure that everything is where it should be, pretty please.

Something you can do is to make a shell command from this program, or make a .EXE file and make a shortcut to your desktop. Maybe I'll make a binary package.

NOTE: For best usage, please use Tailscale VPN to connect your devices over different wifi hosts. The MagicDNS service can also save you a headache from remembering all of those numbers.