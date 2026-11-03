// Plugin SimHub d'Apex Companion : repères de freinage pour les LEDs du volant.
//
// Apex envoie, en local (UDP 127.0.0.1:47823), à chaque échantillon : progression vers le prochain
// point de freinage de la référence, « freine maintenant », secondes restantes, écart de vitesse.
// Ce plugin les publie en propriétés SimHub :
//   ApexCompanion.BrakeProgress  0 (loin) → 1 (au point de freinage)
//   ApexCompanion.BrakeNow       vrai au point de freinage
//   ApexCompanion.BrakeSeconds   secondes avant le point (99 = aucun)
//   ApexCompanion.SpeedGapKmh    ta vitesse − celle de la référence au point (km/h)
//   ApexCompanion.Connected      vrai tant qu'Apex envoie des données
//
// Compilé sur ton PC par Apex avec le compilateur C# de Windows (C# 5) : pas de syntaxe récente ici.
using System;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using GameReaderCommon;
using SimHub.Plugins;

namespace ApexCompanionSimHub
{
    [PluginDescription("Repères de freinage d'Apex Companion pour les LEDs du volant")]
    [PluginAuthor("Apex Companion")]
    [PluginName("Apex Companion")]
    public class ApexCompanion : IPlugin, IDataPlugin
    {
        public const int Port = 47823;

        public PluginManager PluginManager { get; set; }

        private readonly object sync = new object();
        private UdpClient udp;
        private Thread thread;
        private volatile bool running;
        private double progress;
        private double seconds = 99;
        private double gap;
        private bool brakeNow;
        private DateTime last = DateTime.MinValue;
        private string error = "";

        private bool Fresh()
        {
            lock (sync)
            {
                return (DateTime.UtcNow - last).TotalMilliseconds < 500;
            }
        }

        public void Init(PluginManager pluginManager)
        {
            this.AttachDelegate("BrakeProgress", () => { lock (sync) { return Fresh() ? progress : 0.0; } });
            this.AttachDelegate("BrakeNow", () => { lock (sync) { return Fresh() && brakeNow; } });
            this.AttachDelegate("BrakeSeconds", () => { lock (sync) { return Fresh() ? seconds : 99.0; } });
            this.AttachDelegate("SpeedGapKmh", () => { lock (sync) { return Fresh() ? gap : 0.0; } });
            this.AttachDelegate("Connected", () => Fresh());
            this.AttachDelegate("Error", () => { lock (sync) { return error; } });
            try
            {
                udp = new UdpClient(new IPEndPoint(IPAddress.Loopback, Port));
                running = true;
                thread = new Thread(Loop);
                thread.IsBackground = true;
                thread.Name = "ApexCompanion";
                thread.Start();
            }
            catch (Exception e)
            {
                // Port déjà pris (deux SimHub ouverts, autre logiciel) : visible dans la propriété Error.
                lock (sync) { error = "port " + Port + " indisponible : " + e.Message; }
            }
        }

        private void Loop()
        {
            IPEndPoint from = new IPEndPoint(IPAddress.Any, 0);
            while (running)
            {
                try
                {
                    byte[] b = udp.Receive(ref from);
                    string[] f = Encoding.ASCII.GetString(b).Split(';');
                    if (f.Length < 5 || f[0] != "APEX1") continue;
                    CultureInfo inv = CultureInfo.InvariantCulture;
                    double p = double.Parse(f[1], inv), s = double.Parse(f[3], inv), g = double.Parse(f[4], inv);
                    lock (sync)
                    {
                        progress = p;
                        brakeNow = f[2] == "1";
                        seconds = s;
                        gap = g;
                        last = DateTime.UtcNow;
                    }
                }
                catch (SocketException)
                {
                    if (!running) return;
                }
                catch (ObjectDisposedException)
                {
                    return;
                }
                catch (FormatException)
                {
                }
            }
        }

        public void DataUpdate(PluginManager pluginManager, ref GameData data)
        {
        }

        public void End(PluginManager pluginManager)
        {
            running = false;
            if (udp != null) udp.Close();
        }
    }
}
