mod create;
mod delete;
mod list;
mod helpers;

use structopt::StructOpt;

use create::CreateManagedSpuGroupOpt;
use create::process_create_managed_spu_group;

use delete::DeleteManagedSpuGroupOpt;
use delete::process_delete_managed_spu_group;

use list::ListManagedSpuGroupsOpt;
use list::process_list_managed_spu_groups;

use crate::error::CliError;

#[derive(Debug, StructOpt)]
pub enum SpuGroupOpt {
    #[structopt(name = "create", author = "", template = "{about}

{usage}

{all-args}
", about = "Create managed SPU group")]
    Create(CreateManagedSpuGroupOpt),

    #[structopt(name = "delete", author = "", template = "{about}

{usage}

{all-args}
", about = "Delete managed SPU group")]
    Delete(DeleteManagedSpuGroupOpt),

    #[structopt(name = "list", author = "", template = "{about}

{usage}

{all-args}
", about = "List managed SPU groups")]
    List(ListManagedSpuGroupsOpt),
}

pub(crate) fn process_spu_group(spu_group_opt: SpuGroupOpt) -> Result<(), CliError> {
    match spu_group_opt {
        SpuGroupOpt::Create(spu_group_opt) => process_create_managed_spu_group(spu_group_opt),
        SpuGroupOpt::Delete(spu_group_opt) => process_delete_managed_spu_group(spu_group_opt),
        SpuGroupOpt::List(spu_group_opt) => process_list_managed_spu_groups(spu_group_opt),
    }
}
