**Rust-powered LAN file sharing** – fast, private, and under your control.
Take back control of sending and receiving data across devices without relying on
cloud services or slow proprietary apps.
Before **rShare**, sharing large files often meant:
- Clunky email attachments
- Limited services like Opera GX MyFlow (one device pair, auto-deletes files)
- Slow uploads to Google Drive or other clouds

rShare fixes that with a **direct LAN connection** for instant, reliable transfers.

**HOW TO USE:**

Extract binary or clone repo and run.

*If that does not work, follow the troubleshooting steps below.*

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




NOTE: For best usage, please use Tailscale VPN to connect your devices over different wifi hosts. The MagicDNS service can also save you a headache from remembering all of those numbers.

Update 2.11.2026 -- The pretty update. Made the app prettier, easier to read and to understand.

Update 2.12.2026 -- The Foolproof Update. Made the app fool proof for a friend.