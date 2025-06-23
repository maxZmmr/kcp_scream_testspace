import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import os

def plot_scream_performance():
    """
    Reads SCReAM performance data from scream_log.csv and generates a plot.
    """
    try:
        # Laden der Daten aus der CSV-Datei
        df = pd.read_csv('kcp_scream_testspace/scream_log.csv')

        # Überprüfen, ob die Datei leer ist oder nur Header hat
        if df.empty:
            print("Error: scream_log.csv is empty or contains only headers.")
            return

        # Konvertieren des Zeitstempels von Millisekunden in ein Datetime-Objekt für die Darstellung
        df['timestamp'] = pd.to_datetime(df['timestamp_ms'], unit='ms')

        # Erstellen einer Figur mit 3 untereinanderliegenden Graphen
        fig, (ax1, ax2, ax3) = plt.subplots(3, 1, figsize=(15, 12), sharex=True)
        fig.suptitle('SCReAM Congestion Control Behavior', fontsize=16)

        # Graph 1: RTT im Zeitverlauf
        ax1.plot(df['timestamp'], df['rtt_ms'], label='RTT (ms)', color='blue', marker='.', linestyle='-', markersize=4)
        ax1.set_ylabel('RTT (ms)')
        ax1.set_title('Round-Trip Time')
        ax1.grid(True)
        ax1.legend()

        # Graph 2: Ziel-Bitrate im Zeitverlauf
        ax2.plot(df['timestamp'], df['bitrate_kbps'], label='Target Bitrate (kbps)', color='green', marker='.', linestyle='-', markersize=4)
        ax2.set_ylabel('Bitrate (kbps)')
        ax2.set_title('Target Bitrate')
        ax2.grid(True)
        ax2.legend()

        # Graph 3: Congestion Window im Zeitverlauf
        ax3.plot(df['timestamp'], df['cwnd_bytes'], label='CWND (Bytes)', color='red', marker='.', linestyle='-', markersize=4)
        ax3.set_ylabel('CWND (Bytes)')
        ax3.set_title('Calculated Congestion Window')
        ax3.grid(True)
        ax3.legend()

        # Formatierung der X-Achse zur Anzeige der Uhrzeit
        ax3.set_xlabel('Time')
        ax3.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        fig.autofmt_xdate()

        # Anpassen des Layouts und Speichern des Graphen
        plt.tight_layout(rect=[0, 0, 1, 0.96])
        plt.savefig('scream_performance_plot.png')

        os.remove('kcp_scream_testspace/scream_log.csv')
        
        print("Grafik wurde erfolgreich als 'scream_performance_plot.png' gespeichert.")

    except FileNotFoundError:
        print("Fehler: Die Datei 'scream_log.csv' wurde nicht gefunden.")
        print("Bitte führen Sie zuerst das Rust-Testprogramm aus, um die Datendatei zu erzeugen.")
    except Exception as e:
        print(f"Ein unerwarteter Fehler ist aufgetreten: {e}")

if __name__ == '__main__':
    plot_scream_performance()