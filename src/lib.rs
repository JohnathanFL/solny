#![feature(default_free_fn)]

use std::{
    default::default,
    fmt::{Display, Formatter},
    path::{Component, Path, PathBuf},
};

use parse_display::Display;
use thiserror::Error;
use uuid::Uuid;

pub mod type_guids;

#[derive(Debug, Clone)]
pub struct SlnHeader {
    pub format: String,
    pub last_version: String,
    pub vs_version: String,
    pub min_vs_version: String,
}

#[derive(Debug, Clone, Display)]
pub enum SlnLifecycle {
    #[display("preSolution")]
    PreSolution,
    #[display("postSolution")]
    PostSolution,
}

#[derive(Debug, Clone, Display)]
pub enum ProjLifecycle {
    #[display("preProject")]
    PreProject,
    #[display("postProject")]
    PostProject,
}

#[derive(Debug, Clone)]
pub struct GlobalSection {
    pub name: String,
    pub lifecycle: SlnLifecycle,
    pub assignments: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ProjectSection {
    pub name: String,
    pub lifecycle: ProjLifecycle,
    pub assignments: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct SlnProject {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub ty: Uuid,
    pub sections: Vec<ProjectSection>,
}

#[derive(Debug, Clone)]
pub struct ProjectCfgPlatform {
    pub id: Uuid,
    pub cfg: (String, String),
    pub active_cfg: (String, String),
    pub build: (String, String),
}

#[derive(Debug, Clone)]
pub struct Sln {
    pub header: SlnHeader,
    /// Arbitrary global sections.
    pub globals: Vec<GlobalSection>,
    /// Set of (proj, nested_under).
    pub nestings: Vec<(Uuid, Uuid)>,
    pub sln_props: Vec<(String, String)>,
    /// Set of (Configuration, Platform).
    pub sln_cfg_platforms: Vec<(String, String)>,
    /// Set of (Configuration, Platform).
    pub proj_cfg_platforms: Vec<ProjectCfgPlatform>,
    pub projs: Vec<SlnProject>,
}

impl Default for Sln {
    fn default() -> Self {
        Self {
            header: SlnHeader {
                // TODO: Is there a more sensible set of defaults here?
                format: "Microsoft Visual Studio Solution File, Format Version 12.00".into(),
                last_version: "Visual Studio Version 17".into(),
                vs_version: "17.0.31903.59".into(),
                min_vs_version: "10.0.40219.1".into(),
            },
            nestings: Vec::new(),
            sln_props: vec![("HideSolutionNode".into(), "FALSE".into())],
            sln_cfg_platforms: vec![("Debug".into(), "Any CPU".into()), ("Release".into(), "Any CPU".into())],
            proj_cfg_platforms: Vec::new(),
            projs: Vec::new(),
            globals: Vec::new(),
        }
    }
}

#[derive(Error, Debug, Copy, Clone)]
pub enum SlnDirError {
    #[error("Sln dirs must be relative paths")]
    NotRelative,
    #[error("Sln dirs must be relative paths composed of only normal elements (i.e a/b/c)")]
    NotNormal,
}

#[derive(Error, Debug, Copy, Clone)]
pub enum AddProjError {
    #[error(transparent)]
    SlnDirError(#[from] SlnDirError),
}

impl Sln {
    pub fn nesting_of(&self, proj_id: Uuid) -> Option<Uuid> {
        self.nestings
            .iter()
            .find(|(x, y)| *x == proj_id)
            .map(|(_, y)| y)
            .copied()
    }

    pub fn find_proj_id<S: AsRef<str>>(&self, name: S, ty: Uuid, parent: Option<Uuid>) -> Option<Uuid> {
        self.projs
            .iter()
            .find(|p| p.ty == ty && p.name == name.as_ref() && self.nesting_of(p.id) == parent)
            .map(|p| p.id)
    }

    /// Ensure the sln contains the requisite [solution dir](type_guids::SOLUTION_FOLDER)
    /// projects for a given path and return the guid to nest under.
    pub fn ensure_sln_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<Uuid, SlnDirError> {
        let mut parent: Option<Uuid> = None;
        for level in path.as_ref().components() {
            match level {
                Component::Prefix(_) | Component::RootDir => return Err(SlnDirError::NotRelative),
                Component::CurDir | Component::ParentDir => return Err(SlnDirError::NotNormal),
                Component::Normal(e) => {
                    // The existing one is the "project" that is a solution folder
                    // with a matching name and who is nested under the current parent (or no parent)
                    let existing = self.find_proj_id(e.to_str().unwrap(), type_guids::SOLUTION_FOLDER, parent);
                    parent = if let Some(e) = existing {
                        Some(e)
                    } else {
                        let id = Uuid::new_v4();
                        let name = e.to_str().unwrap().to_string();
                        self.projs.push(SlnProject {
                            id,
                            ty: type_guids::SOLUTION_FOLDER,
                            path: name.clone().into(),
                            name,
                            ..default()
                        });
                        if let Some(p) = parent {
                            self.nestings.push((id, p));
                        }

                        Some(id)
                    };
                }
            }
        }

        Ok(parent.unwrap())
    }

    pub fn add_proj<P1: AsRef<Path>, P2: AsRef<Path>>(
        &mut self,
        name: String,
        proj_path: P1,
        ty: Uuid,
        sln_dir: Option<P2>,
    ) -> Result<Uuid, AddProjError> {
        let parent_dir = match sln_dir.map(|p| self.ensure_sln_dir(p)) {
            Some(r) => Some(r?),
            None => None,
        };

        if let Some(id) = self.find_proj_id(&name, ty, parent_dir) {
            Ok(id)
        } else {
            let id = Uuid::new_v4();
            self.projs.push(SlnProject {
                id,
                name,
                ty,
                path: proj_path.as_ref().into(),
                ..default()
            });
            if let Some(p) = parent_dir {
                self.nestings.push((id, p));
            }
            Ok(id)
        }
    }

    pub fn add_csproj<P1: AsRef<Path>, P2: AsRef<Path>>(
        &mut self,
        name: String,
        proj_path: P1,
        sln_dir: Option<P2>,
    ) -> Result<Uuid, AddProjError> {
        let id = self.add_proj(name, proj_path, type_guids::CSHARP, sln_dir)?;

        if !self.proj_cfg_platforms.iter().any(|c| c.id == id) {
            for (cfg, plat) in &self.sln_cfg_platforms {
                self.proj_cfg_platforms.push(ProjectCfgPlatform {
                    id,
                    active_cfg: (cfg.clone(), plat.clone()),
                    build: (cfg.clone(), plat.clone()),
                    cfg: (cfg.clone(), plat.clone()),
                })
            }
        }

        self.projs.sort_by(|p1, p2| p1.ty.cmp(&p2.ty));

        Ok(id)
    }
}

impl Display for Sln {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.header.format)?;
        writeln!(f, "# {}", self.header.last_version)?;
        writeln!(f, "VisualStudioVersion = {}", self.header.vs_version)?;
        writeln!(f, "MinimumVisualStudioVersion = {}", self.header.min_vs_version)?;

        for p in &self.projs {
            writeln!(
                f,
                r#"Project("{}") = "{}", "{}", "{}""#,
                p.ty.as_braced(),
                p.name,
                p.path.display(),
                p.id.as_braced()
            )?;
            for ps in &p.sections {
                writeln!(f, "  ProjectSection({})", ps.name)?;
                for (lhs, rhs) in &ps.assignments {
                    writeln!(f, "    {lhs} = {rhs}")?;
                }
                writeln!(f, "  EndProjectSection")?;
            }
            writeln!(f, "EndProject")?;
        }

        writeln!(f, "Global")?;
        {
            writeln!(
                f,
                "  GlobalSection(SolutionConfigurationPlatforms) = {}",
                SlnLifecycle::PreSolution
            )?;
            for (x, y) in &self.sln_cfg_platforms {
                writeln!(f, "    {x}|{y} = {x}|{y}")?;
            }
            writeln!(f, "  EndGlobalSection")?;

            writeln!(f, "  GlobalSection(SolutionProperties) = {}", SlnLifecycle::PreSolution)?;
            for (x, y) in &self.sln_props {
                writeln!(f, "    {} = {}", x, y)?;
            }
            writeln!(f, "  EndGlobalSection")?;

            writeln!(
                f,
                "  GlobalSection(ProjectConfigurationPlatforms) = {}",
                SlnLifecycle::PreSolution
            )?;
            for cfg in &self.proj_cfg_platforms {
                writeln!(
                    f,
                    "    {}.{}|{}.ActiveCfg = {}|{}",
                    cfg.id.as_braced(),
                    cfg.cfg.0,
                    cfg.cfg.1,
                    cfg.active_cfg.0,
                    cfg.active_cfg.1
                )?;
                writeln!(
                    f,
                    "    {}.{}|{}.Build.0 = {}|{}",
                    cfg.id.as_braced(),
                    cfg.cfg.0,
                    cfg.cfg.1,
                    cfg.build.0,
                    cfg.build.1
                )?;
            }
            writeln!(f, "  EndGlobalSection")?;

            writeln!(f, "  GlobalSection(NestedProjects) = {}", SlnLifecycle::PreSolution)?;
            for (x, y) in &self.nestings {
                writeln!(f, "    {} = {}", x.as_braced(), y.as_braced())?;
            }
            writeln!(f, "  EndGlobalSection")?;

            for g in &self.globals {
                writeln!(f, "  GlobalSection({}) = {}", g.name, g.lifecycle)?;
                for (lhs, rhs) in &g.assignments {
                    writeln!(f, "    {lhs} = {rhs}")?;
                }
                writeln!(f, "  EndGlobalSection")?;
            }
        }
        writeln!(f, "EndGlobal")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() -> Result<(), AddProjError> {
        let mut sln = Sln::default();
        sln.add_csproj("testf1".into(), "f1/testf1/testf1.csproj", Some("f1"))?;
        sln.add_csproj("testf2".into(), "f1/f2/testf2/testf2.csproj", Some("f1/f2"))?;
        sln.add_csproj("testf1f2".into(), "f1/f2/f1/testf1f2/testf1f2.csproj", Some("f1/f2/f1"))?;

        println!("{sln}");

        Ok(())
    }
}
