import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import os

def plot_scream_performance_with_loss():
    """
    Reads SCReAM performance data and plots it, highlighting packet loss events.
    """
    try:
        df = pd.read_csv('kcp_scream_testspace/scream_log.csv')
        if df.empty:
            print("Error: scream_log.csv is empty.")
            return

        df['timestamp'] = pd.to_datetime(df['timestamp_ms'], unit='ms')

        loss_events = df[df['packet_loss'] == 1]

        # Erstellen der Graphen
        fig, (ax1, ax2, ax3) = plt.subplots(3, 1, figsize=(15, 12), sharex=True)
        fig.suptitle('SCReAM Congestion Control Behavior with Loss Indicators', fontsize=16)

        # Graph 1: RTT
        ax1.plot(df['timestamp'], df['rtt_ms'], label='RTT (ms)', color='blue', alpha=0.7)
        ax1.set_ylabel('RTT (ms)')
        ax1.set_title('Round-Trip Time')
        ax1.grid(True)
        ax1.legend()

        # Graph 2: Ziel-Bitrate
        ax2.plot(df['timestamp'], df['bitrate_kbps'], label='Target Bitrate (kbps)', color='green')
        ax2.scatter(loss_events['timestamp'], loss_events['bitrate_kbps'], 
                    color='gold', 
                    edgecolor='black',
                    s=100, 
                    zorder=5, 
                    label='Packet Loss Event')
        ax2.set_ylabel('Bitrate (kbps)')
        ax2.set_title('Target Bitrate')
        ax2.grid(True)
        ax2.legend()

        # Graph 3: Congestion Window
        ax3.plot(df['timestamp'], df['cwnd_bytes'], label='CWND (Bytes)', color='red', alpha=0.8)
        ax3.set_ylabel('CWND (Bytes)')
        ax3.set_title('Calculated Congestion Window')
        ax3.grid(True)
        ax3.legend()

        # Formatierung der X-Achse
        ax3.set_xlabel('Time')
        ax3.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        fig.autofmt_xdate()

        plt.tight_layout(rect=[0, 0, 1, 0.96])
        plt.savefig('scream_performance_plot_with_loss.png')
        
        print("Grafik wurde erfolgreich als 'scream_performance_plot_with_loss.png' gespeichert.")

        os.remove("kcp_scream_testspace/scream_log.csv")

    except FileNotFoundError:
        print("Fehler: Die Datei 'scream_log.csv' wurde nicht gefunden.")
        print("Bitte führen Sie zuerst das angepasste Rust-Testprogramm aus, um die neue Datendatei zu erzeugen.")
    except KeyError:
        print("Fehler: Die Spalte 'packet_loss' wurde in 'scream_log.csv' nicht gefunden.")
        print("Stellen Sie sicher, dass Sie die Rust-Anwendung nach den Code-Änderungen neu kompiliert und ausgeführt haben.")
    except Exception as e:
        print(f"Ein unerwarteter Fehler ist aufgetreten: {e}")

if __name__ == '__main__':
    plot_scream_performance_with_loss()