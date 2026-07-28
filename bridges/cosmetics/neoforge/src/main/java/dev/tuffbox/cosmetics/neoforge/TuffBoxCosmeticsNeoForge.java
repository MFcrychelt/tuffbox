package dev.tuffbox.cosmetics.neoforge;

import dev.tuffbox.cosmetics.core.CosmeticsCore;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.loading.FMLEnvironment;
import net.neoforged.fml.loading.FMLPaths;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * NeoForge 1.21.1 entry: loads shared CosmeticsCore session.
 * Full feature renderers ship on Fabric first; NeoForge FX hooks land per MATRIX.md.
 */
@Mod(value = "tuffbox_cosmetics", dist = Dist.CLIENT)
public final class TuffBoxCosmeticsNeoForge {
    private static final Logger LOG = LoggerFactory.getLogger("tuffbox_cosmetics");

    public TuffBoxCosmeticsNeoForge(IEventBus modBus) {
        if (FMLEnvironment.dist != Dist.CLIENT) return;
        try {
            CosmeticsCore.Session session = CosmeticsCore.loadSession(FMLPaths.GAMEDIR.get());
            if (session != null) {
                CosmeticsCore.put(session.username, CosmeticsCore.snapshotFromSession(session));
                LOG.info("TuffBox cosmetics session for {} (NeoForge 1.21.1)", session.username);
            } else {
                LOG.info("TuffBox cosmetics NeoForge — no .tuffbox/cosmetics-session.json");
            }
        } catch (Exception e) {
            LOG.warn("cosmetics neo init: {}", e.toString());
        }
    }
}
